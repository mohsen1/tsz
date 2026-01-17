//! ThinChecker - Type checker using ThinNodeArena and Solver
//!
//! This checker uses the ThinNode architecture for cache-optimized AST access
//! and the Solver's type system for structural type interning.
//!
//! # Architecture
//!
//! - Uses ThinNodeArena for AST access (16-byte cache-optimized nodes)
//! - Uses ThinBinderState for symbol information
//! - Uses Solver's TypeInterner for structural type equality (O(1) comparison)
//! - Uses solver::lower::TypeLower for AST-to-type conversion
//!
//! # Status
//!
//! Phase 7.5 integration - using solver type system for type checking.

use crate::binder::{ContainerKind, ScopeId, SymbolId, symbol_flags};
use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
};
use crate::checker::{CheckerContext, EnclosingClassInfo, FlowAnalyzer};
use crate::interner::Atom;
use crate::parser::syntax_kind_ext;
use crate::parser::thin_node::{ImportDeclData, ThinNodeArena};
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::solver::{ContextualTypeContext, TypeId, TypeInterner};
use crate::thin_binder::ThinBinderState;
use rustc_hash::FxHashSet;
use std::sync::Arc;

// =============================================================================
// ThinCheckerState
// =============================================================================

/// Type checker state using ThinNodeArena and Solver type system.
///
/// This is a performance-optimized checker that works directly with the
/// cache-friendly ThinNode architecture and uses the solver's TypeInterner
/// for structural type equality.
///
/// The state is stored in a `CheckerContext` which can be shared with
/// specialized checker modules (expressions, statements, declarations).
pub struct ThinCheckerState<'a> {
    /// Shared checker context containing all state.
    pub ctx: CheckerContext<'a>,
}

/// Maximum depth for recursive type instantiation.
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum depth for call expression resolution.
pub const MAX_CALL_DEPTH: u32 = 20;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum EnumKind {
    Numeric,
    String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum MemberAccessLevel {
    Private,
    Protected,
}

#[derive(Clone, Debug)]
struct MemberAccessInfo {
    level: MemberAccessLevel,
    declaring_class_idx: NodeIndex,
    declaring_class_name: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum MemberLookup {
    NotFound,
    Public,
    Restricted(MemberAccessLevel),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum PropertyKey {
    Ident(String),
    Private(String),
    Computed(ComputedKey),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum ComputedKey {
    Ident(String),
    String(String),
    Number(String),
    Qualified(String),
    /// Symbol call like Symbol("key") or Symbol() - stores optional description
    Symbol(Option<String>),
}

#[derive(Clone, Debug)]
struct FlowResult {
    normal: Option<FxHashSet<PropertyKey>>,
    exits: Option<FxHashSet<PropertyKey>>,
}

impl<'a> ThinCheckerState<'a> {
    /// Create a new ThinCheckerState.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `binder` - The binder state with symbols
    /// * `types` - The shared type interner (for thread-safe type deduplication)
    /// * `file_name` - The source file name
    /// * `strict` - Whether strict mode is enabled (controls noImplicitAny, etc.)
    pub fn new(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
        types: &'a TypeInterner,
        file_name: String,
        strict: bool,
    ) -> Self {
        ThinCheckerState {
            ctx: CheckerContext::new(arena, binder, types, file_name, strict),
        }
    }

    /// Create a new ThinCheckerState with a persistent cache.
    /// This allows reusing type checking results from previous queries.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `binder` - The binder state with symbols
    /// * `types` - The shared type interner
    /// * `file_name` - The source file name
    /// * `cache` - The persistent type cache from previous queries
    /// * `strict` - Whether strict mode is enabled (controls noImplicitAny, etc.)
    pub fn with_cache(
        arena: &'a ThinNodeArena,
        binder: &'a ThinBinderState,
        types: &'a TypeInterner,
        file_name: String,
        cache: crate::checker::TypeCache,
        strict: bool,
    ) -> Self {
        ThinCheckerState {
            ctx: CheckerContext::with_cache(arena, binder, types, file_name, cache, strict),
        }
    }

    /// Extract the persistent cache from this checker.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> crate::checker::TypeCache {
        self.ctx.extract_cache()
    }

    // =========================================================================
    // Symbol Type Caching
    // =========================================================================

    /// Cache a computed symbol type for stateless lookup.
    fn cache_symbol_type(&mut self, sym_id: SymbolId, type_id: TypeId) {
        self.ctx.symbol_types.insert(sym_id, type_id);
    }

    fn record_symbol_dependency(&mut self, dependency: SymbolId) {
        let Some(&current) = self.ctx.symbol_dependency_stack.last() else {
            return;
        };
        if current == dependency {
            return;
        }
        self.ctx
            .symbol_dependencies
            .entry(current)
            .or_default()
            .insert(dependency);
    }

    fn push_symbol_dependency(&mut self, sym_id: SymbolId, clear_deps: bool) {
        if clear_deps {
            self.ctx.symbol_dependencies.remove(&sym_id);
        }
        self.ctx.symbol_dependency_stack.push(sym_id);
    }

    fn pop_symbol_dependency(&mut self) {
        self.ctx.symbol_dependency_stack.pop();
    }

    fn cache_parameter_types(
        &mut self,
        params: &[NodeIndex],
        param_types: Option<&[Option<TypeId>]>,
    ) {
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(param.name)
                .or_else(|| self.ctx.binder.get_node_symbol(param_idx))
            else {
                continue;
            };
            self.push_symbol_dependency(sym_id, true);
            let type_id = if let Some(types) = param_types {
                types.get(i).and_then(|t| *t)
            } else if !param.type_annotation.is_none() {
                Some(self.get_type_from_type_node(param.type_annotation))
            } else {
                // Return UNKNOWN instead of ANY for parameter without type annotation
                Some(TypeId::UNKNOWN)
            };
            self.pop_symbol_dependency();

            if let Some(type_id) = type_id {
                self.cache_symbol_type(sym_id, type_id);
            }
        }
    }

    /// Push an expected return type onto the stack (when entering a function).
    pub fn push_return_type(&mut self, return_type: TypeId) {
        self.ctx.push_return_type(return_type);
    }

    /// Pop an expected return type from the stack (when exiting a function).
    pub fn pop_return_type(&mut self) {
        self.ctx.pop_return_type();
    }

    /// Get the current expected return type (if in a function).
    pub fn current_return_type(&self) -> Option<TypeId> {
        self.ctx.current_return_type()
    }

    // =========================================================================
    // Diagnostics (delegated to CheckerContext)
    // =========================================================================

    /// Add an error diagnostic.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        self.ctx.error(start, length, message, code);
    }

    /// Get node span (pos, end) from index.
    pub fn get_node_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        self.ctx.get_node_span(idx)
    }

    // =========================================================================
    // Symbol Resolution
    // =========================================================================

    /// Get the symbol for a node index.
    pub fn get_symbol_at_node(&self, idx: NodeIndex) -> Option<SymbolId> {
        self.ctx.binder.get_node_symbol(idx)
    }

    /// Get the symbol by name from file locals.
    pub fn get_symbol_by_name(&self, name: &str) -> Option<SymbolId> {
        self.ctx.binder.file_locals.get(name)
    }

    fn is_class_member_symbol(flags: u32) -> bool {
        (flags
            & (symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR
                | symbol_flags::CONSTRUCTOR))
            != 0
    }

    fn find_enclosing_scope(&self, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;
        while !current.is_none() {
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        // Only fall back to ScopeId(0) if it's a valid module scope
        // This prevents using an invalid fallback scope that could cause
        // symbols to be incorrectly found or not found
        if let Some(scope) = self.ctx.binder.scopes.get(0) {
            // Only return ScopeId(0) if it's a module scope (the global/file scope)
            if scope.kind == ContainerKind::Module {
                return Some(ScopeId(0));
            }
        }
        None
    }

    fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        let name = self.ctx.arena.get_identifier(node)?.escaped_text.as_str();

        // Collect lib binders for cross-arena symbol lookup
        let lib_binders: Vec<Arc<crate::thin_binder::ThinBinderState>> =
            self.ctx.lib_contexts.iter().map(|lc| Arc::clone(&lc.binder)).collect();

        let debug = std::env::var("BIND_DEBUG").is_ok();

        // === PHASE 1: Initial logging ===
        if debug {
            eprintln!("\n[BIND_RESOLVE] ========================================");
            eprintln!("[BIND_RESOLVE] Looking up identifier '{}'", name);
            eprintln!("[BIND_RESOLVE]   Node index: {:?}", idx);
            eprintln!("[BIND_RESOLVE]   Lib contexts available: {}", self.ctx.lib_contexts.len());
            eprintln!("[BIND_RESOLVE]   Lib binders collected: {}", lib_binders.len());
            eprintln!("[BIND_RESOLVE]   Total scopes in binder: {}", self.ctx.binder.scopes.len());
            eprintln!("[BIND_RESOLVE]   file_locals size: {}", self.ctx.binder.file_locals.len());
        }

        // === PHASE 2: Scope chain traversal (local -> parent -> ... -> module) ===
        if let Some(mut scope_id) = self.find_enclosing_scope(idx) {
            if debug {
                eprintln!("[BIND_RESOLVE] Starting scope chain from: {:?}", scope_id);
            }
            let require_export = false;
            let mut scope_depth = 0;
            while !scope_id.is_none() {
                scope_depth += 1;
                if let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) {
                    if debug {
                        eprintln!("[BIND_RESOLVE]   [Scope {}] id={:?}, kind={:?}, parent={:?}, table_size={}",
                            scope_depth, scope_id, scope.kind, scope.parent, scope.table.len());
                    }

                    // Check scope's local symbol table
                    if let Some(sym_id) = scope.table.get(name) {
                        if debug {
                            eprintln!("[BIND_RESOLVE]     -> Found '{}' in scope table as {:?}", name, sym_id);
                        }
                        // Use get_symbol_with_libs to check lib binders
                        if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                            let export_ok = !require_export
                                || scope.kind != ContainerKind::Module
                                || symbol.is_exported
                                || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                            let is_class_member = Self::is_class_member_symbol(symbol.flags);
                            if debug {
                                eprintln!("[BIND_RESOLVE]        Symbol flags: 0x{:x}", symbol.flags);
                                eprintln!("[BIND_RESOLVE]        is_exported: {}, export_ok: {}, is_class_member: {}",
                                    symbol.is_exported, export_ok, is_class_member);
                            }
                            if export_ok && !is_class_member {
                                if debug {
                                    eprintln!("[BIND_RESOLVE]     -> SUCCESS: Returning {:?} from scope {:?}", sym_id, scope_id);
                                }
                                return Some(sym_id);
                            } else if debug {
                                eprintln!("[BIND_RESOLVE]        SKIPPED: export_ok={}, is_class_member={}", export_ok, is_class_member);
                            }
                        } else if !require_export || scope.kind != ContainerKind::Module {
                            if debug {
                                eprintln!("[BIND_RESOLVE]     -> SUCCESS: Found '{}' in scope {:?} (no symbol data, returning anyway)", name, scope_id);
                            }
                            return Some(sym_id);
                        } else if debug {
                            eprintln!("[BIND_RESOLVE]        SKIPPED: No symbol data and require_export or module scope");
                        }
                    }

                    // Check module exports
                    if scope.kind == ContainerKind::Module {
                        if debug {
                            eprintln!("[BIND_RESOLVE]     Checking module exports (container_node: {:?})", scope.container_node);
                        }
                        if let Some(container_sym_id) =
                            self.ctx.binder.get_node_symbol(scope.container_node)
                        {
                            if debug {
                                eprintln!("[BIND_RESOLVE]       Container symbol: {:?}", container_sym_id);
                            }
                            if let Some(container_symbol) =
                                self.ctx.binder.get_symbol_with_libs(container_sym_id, &lib_binders)
                            {
                                if let Some(exports) = container_symbol.exports.as_ref() {
                                    if debug {
                                        eprintln!("[BIND_RESOLVE]       Module has {} exports", exports.len());
                                    }
                                    if let Some(member_id) = exports.get(name) {
                                        if debug {
                                            eprintln!("[BIND_RESOLVE]       -> Found '{}' in exports as {:?}", name, member_id);
                                        }
                                        if let Some(member_symbol) =
                                            self.ctx.binder.get_symbol_with_libs(member_id, &lib_binders)
                                        {
                                            let is_class_member = Self::is_class_member_symbol(member_symbol.flags);
                                            if debug {
                                                eprintln!("[BIND_RESOLVE]          Member flags: 0x{:x}, is_class_member: {}",
                                                    member_symbol.flags, is_class_member);
                                            }
                                            if !is_class_member {
                                                if debug {
                                                    eprintln!("[BIND_RESOLVE]       -> SUCCESS: Returning {:?} from module exports", member_id);
                                                }
                                                return Some(member_id);
                                            }
                                        } else {
                                            if debug {
                                                eprintln!("[BIND_RESOLVE]       -> SUCCESS: Found '{}' in module exports (no symbol data)", name);
                                            }
                                            return Some(member_id);
                                        }
                                    }
                                } else if debug {
                                    eprintln!("[BIND_RESOLVE]       Container has no exports");
                                }
                            } else if debug {
                                eprintln!("[BIND_RESOLVE]       Could not get container symbol data");
                            }
                        } else if debug {
                            eprintln!("[BIND_RESOLVE]       No container symbol for module");
                        }
                    }

                    let parent_id = scope.parent;
                    // Nested namespaces can reference non-exported parent members (TSC behavior).
                    scope_id = parent_id;
                } else {
                    if debug {
                        eprintln!("[BIND_RESOLVE]   [Scope {}] INVALID scope_id={:?} - breaking", scope_depth, scope_id);
                    }
                    break;
                }
            }
            if debug {
                eprintln!("[BIND_RESOLVE] Exhausted scope chain after {} scopes", scope_depth);
            }
        } else if debug {
            eprintln!("[BIND_RESOLVE] No enclosing scope found for node {:?}", idx);
        }

        // === PHASE 3: Check file_locals (global scope from lib.d.ts) ===
        if debug {
            eprintln!("[BIND_RESOLVE] Checking file_locals ({} symbols)", self.ctx.binder.file_locals.len());
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            if debug {
                eprintln!("[BIND_RESOLVE]   -> Found '{}' in file_locals as {:?}", name, sym_id);
            }
            // Use get_symbol_with_libs to check lib binders
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                let is_class_member = Self::is_class_member_symbol(symbol.flags);
                if debug {
                    eprintln!("[BIND_RESOLVE]      Symbol flags: 0x{:x}, is_class_member: {}", symbol.flags, is_class_member);
                }
                if !is_class_member {
                    if debug {
                        eprintln!("[BIND_RESOLVE]   -> SUCCESS: Returning {:?} from file_locals", sym_id);
                    }
                    return Some(sym_id);
                } else if debug {
                    eprintln!("[BIND_RESOLVE]      SKIPPED: is_class_member");
                }
            } else {
                if debug {
                    eprintln!("[BIND_RESOLVE]   -> SUCCESS: Found '{}' in file_locals (no symbol data)", name);
                }
                return Some(sym_id);
            }
        }

        // === PHASE 4: Check lib binders' file_locals directly ===
        if debug {
            eprintln!("[BIND_RESOLVE] Checking {} lib binders' file_locals...", lib_binders.len());
        }
        for (i, lib_binder) in lib_binders.iter().enumerate() {
            if debug {
                eprintln!("[BIND_RESOLVE]   [Lib {}] file_locals size: {}", i, lib_binder.file_locals.len());
            }
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                if debug {
                    eprintln!("[BIND_RESOLVE]     -> Found '{}' in lib binder {} as {:?}", name, i, sym_id);
                }

                // Try to get symbol data with cross-arena resolution
                // This handles cases where lib symbols reference other arenas
                let symbol_opt = lib_binder.get_symbol_with_libs(sym_id, &lib_binders);

                if let Some(symbol) = symbol_opt {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if debug {
                        eprintln!("[BIND_RESOLVE]        Symbol flags: 0x{:x}, is_class_member: {}", symbol.flags, is_class_member);
                    }
                    // For lib binders, be more permissive with class members
                    // Intrinsic types (Object, Array, etc.) may have class member flags
                    // but should still be accessible as global values
                    if !is_class_member || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0 {
                        if debug {
                            eprintln!("[BIND_RESOLVE]     -> SUCCESS: Returning {:?} from lib binder {}", sym_id, i);
                        }
                        return Some(sym_id);
                    } else if debug {
                        eprintln!("[BIND_RESOLVE]        SKIPPED: is_class_member without EXPORT_VALUE");
                    }
                } else {
                    // No symbol data available - return sym_id anyway
                    // This handles cross-arena references and ambient declarations
                    if debug {
                        eprintln!("[BIND_RESOLVE]     -> SUCCESS: Found '{}' in lib binder {} (no symbol data)", name, i);
                    }
                    return Some(sym_id);
                }
            }
        }

        // === PHASE 5: Symbol not found - diagnostic dump ===
        if debug {
            eprintln!("[BIND_RESOLVE] FAILED: '{}' NOT FOUND in any location", name);
            eprintln!("[BIND_RESOLVE] Diagnostic dump:");
            eprintln!("[BIND_RESOLVE]   - Searched {} scope chain levels",
                self.find_enclosing_scope(idx).map_or(0, |s| {
                    let mut count = 0;
                    let mut sid = s;
                    while !sid.is_none() {
                        if let Some(scope) = self.ctx.binder.scopes.get(sid.0 as usize) {
                            count += 1;
                            sid = scope.parent;
                        } else {
                            break;
                        }
                    }
                    count
                }));
            eprintln!("[BIND_RESOLVE]   - Searched file_locals ({} entries)", self.ctx.binder.file_locals.len());
            eprintln!("[BIND_RESOLVE]   - Searched {} lib binders", lib_binders.len());

            // Dump file_locals for debugging (if not too large)
            if self.ctx.binder.file_locals.len() < 50 {
                eprintln!("[BIND_RESOLVE]   Main binder file_locals:");
                for (n, id) in self.ctx.binder.file_locals.iter() {
                    eprintln!("     - {} -> {:?}", n, id);
                }
            } else {
                eprintln!("[BIND_RESOLVE]   (file_locals too large to dump: {} entries)", self.ctx.binder.file_locals.len());
            }

            // Sample lib binder file_locals
            for (i, lib_binder) in lib_binders.iter().enumerate() {
                if lib_binder.file_locals.len() < 30 {
                    eprintln!("[BIND_RESOLVE]   Lib binder {} file_locals:", i);
                    for (n, id) in lib_binder.file_locals.iter() {
                        eprintln!("     - {} -> {:?}", n, id);
                    }
                } else {
                    eprintln!("[BIND_RESOLVE]   Lib binder {} has {} file_locals (sampling first 10):", i, lib_binder.file_locals.len());
                    for (n, id) in lib_binder.file_locals.iter().take(10) {
                        eprintln!("     - {} -> {:?}", n, id);
                    }
                }
            }
            eprintln!("[BIND_RESOLVE] ========================================\n");
        }

        None
    }

    fn resolve_private_identifier_symbols(&self, idx: NodeIndex) -> (Vec<SymbolId>, bool) {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return (Vec::new(), false),
        };
        let name = match self.ctx.arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return (Vec::new(), false),
        };

        let mut symbols = Vec::new();
        let mut saw_class_scope = false;
        let Some(mut scope_id) = self.find_enclosing_scope(idx) else {
            return (symbols, saw_class_scope);
        };

        while !scope_id.is_none() {
            let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
                break;
            };
            if scope.kind == ContainerKind::Class {
                saw_class_scope = true;
            }
            if let Some(sym_id) = scope.table.get(name) {
                symbols.push(sym_id);
            }
            scope_id = scope.parent;
        }

        (symbols, saw_class_scope)
    }

    // =========================================================================
    // Type Resolution - Core Methods
    // =========================================================================

    /// Get the type of a node.
    pub fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }

        // Check for circular reference - return ERROR to expose resolution bugs
        if self.ctx.node_resolution_set.contains(&idx) {
            return TypeId::ERROR;
        }

        // Push onto resolution stack
        self.ctx.node_resolution_stack.push(idx);
        self.ctx.node_resolution_set.insert(idx);

        let result = self.compute_type_of_node(idx);

        // Pop from resolution stack
        self.ctx.node_resolution_stack.pop();
        self.ctx.node_resolution_set.remove(&idx);

        // Cache result
        self.ctx.node_types.insert(idx.0, result);

        result
    }

    /// Compute the type of a node (internal, not cached).
    fn compute_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let is_function_declaration = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;

        match node.kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => self.get_type_of_identifier(idx),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                if let Some(this_type) = self.current_this_type() {
                    this_type
                } else if let Some(ref class_info) = self.ctx.enclosing_class.clone() {
                    // Inside a class but no explicit this type on stack -
                    // return the class instance type (e.g., for constructor default params)
                    if let Some(class_node) = self.ctx.arena.get(class_info.class_idx) {
                        if let Some(class_data) = self.ctx.arena.get_class(class_node) {
                            return self.get_class_instance_type(class_info.class_idx, class_data);
                        }
                    }
                    TypeId::ANY
                } else {
                    // Not in a class - check if we're in a NON-ARROW function
                    // Arrow functions capture `this` from their enclosing scope, so they
                    // should NOT trigger TS2683. We need to skip past arrow functions
                    // to find the actual enclosing function that defines the `this` context.
                    if self.ctx.no_implicit_this && self.find_enclosing_non_arrow_function(idx).is_some() {
                        // TS2683: 'this' implicitly has type 'any'
                        // Only emit when noImplicitThis is enabled
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages,
                        };
                        self.error_at_node(
                            idx,
                            diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY,
                            diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY,
                        );
                        TypeId::ANY
                    } else {
                        // Outside function, only inside arrow functions, or noImplicitThis disabled
                        // Use ANY for recovery without error
                        TypeId::ANY
                    }
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => self.get_type_of_super_keyword(idx),

            // Literals - preserve literal types when contextual typing expects them.
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let literal_type = self.literal_type_from_initializer(idx);
                if let Some(literal_type) = literal_type {
                    if self.contextual_literal_type(literal_type).is_some() {
                        literal_type
                    } else {
                        TypeId::NUMBER
                    }
                } else {
                    TypeId::NUMBER
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let literal_type = self.literal_type_from_initializer(idx);
                if let Some(literal_type) = literal_type {
                    if self.contextual_literal_type(literal_type).is_some() {
                        literal_type
                    } else {
                        TypeId::STRING
                    }
                } else {
                    TypeId::STRING
                }
            }
            k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),
            k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,

            // Binary expressions
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self.get_type_of_binary_expression(idx),

            // Call expressions
            k if k == syntax_kind_ext::CALL_EXPRESSION => self.get_type_of_call_expression(idx),

            // New expressions
            k if k == syntax_kind_ext::NEW_EXPRESSION => self.get_type_of_new_expression(idx),

            // Class expressions
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class) = self.ctx.arena.get_class(node).cloned() {
                    self.check_class_expression(idx, &class);
                    self.get_class_constructor_type(idx, &class)
                } else {
                    // Return UNKNOWN instead of ANY when class expression cannot be resolved
                    TypeId::UNKNOWN
                }
            }

            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.get_type_of_property_access(idx)
            }

            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.get_type_of_element_access(idx)
            }

            // Conditional expression (ternary)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.get_type_of_conditional_expression(idx)
            }

            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.get_type_of_variable_declaration(idx)
            }

            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self.get_type_of_function(idx),

            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => self.get_type_of_function(idx),

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.get_type_of_function(idx),

            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.get_type_of_array_literal(idx)
            }

            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.get_type_of_object_literal(idx)
            }

            // Prefix unary expression
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.get_type_of_prefix_unary(idx)
            }

            // Postfix unary expression - ++ and -- always return number
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => TypeId::NUMBER,

            // typeof expression
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,

            // void expression
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,

            // await expression - unwrap Promise<T> to get T
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    let expr_type = self.get_type_of_node(unary.expression);
                    // If the awaited type is Promise-like, extract the type argument
                    // Otherwise, return UNKNOWN as a fallback (consistent with Task 4-6 changes)
                    self.promise_like_return_type_argument(expr_type)
                        .unwrap_or(TypeId::UNKNOWN)
                } else {
                    // Return UNKNOWN instead of ANY when await expression cannot be resolved
                    TypeId::UNKNOWN
                }
            }

            // Parenthesized expression - just pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.get_type_of_node(paren.expression)
                } else {
                    // Return UNKNOWN instead of ANY when parenthesized expression cannot be resolved
                    TypeId::UNKNOWN
                }
            }

            // Template expression (template literals with substitutions)
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.get_type_of_template_expression(idx)
            }

            // No-substitution template literal - preserve literal type when contextual typing expects it.
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                let literal_type = self.literal_type_from_initializer(idx);
                if let Some(literal_type) = literal_type {
                    if self.contextual_literal_type(literal_type).is_some() {
                        literal_type
                    } else {
                        TypeId::STRING
                    }
                } else {
                    TypeId::STRING
                }
            }

            // =========================================================================
            // Type Nodes
            // =========================================================================

            // Type reference (e.g., "number", "string", "MyType")
            k if k == syntax_kind_ext::TYPE_REFERENCE => self.get_type_from_type_reference(idx),

            // Keyword types
            k if k == SyntaxKind::NumberKeyword as u16 => TypeId::NUMBER,
            k if k == SyntaxKind::StringKeyword as u16 => TypeId::STRING,
            k if k == SyntaxKind::BooleanKeyword as u16 => TypeId::BOOLEAN,
            k if k == SyntaxKind::VoidKeyword as u16 => TypeId::VOID,
            k if k == SyntaxKind::AnyKeyword as u16 => TypeId::ANY,
            k if k == SyntaxKind::NeverKeyword as u16 => TypeId::NEVER,
            k if k == SyntaxKind::UnknownKeyword as u16 => TypeId::UNKNOWN,
            k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if k == SyntaxKind::ObjectKeyword as u16 => TypeId::OBJECT,
            k if k == SyntaxKind::BigIntKeyword as u16 => TypeId::BIGINT,
            k if k == SyntaxKind::SymbolKeyword as u16 => TypeId::SYMBOL,

            // Union type (A | B)
            k if k == syntax_kind_ext::UNION_TYPE => self.get_type_from_union_type(idx),

            // Intersection type (A & B)
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                self.get_type_from_intersection_type(idx)
            }

            // Array type (T[])
            k if k == syntax_kind_ext::ARRAY_TYPE => self.get_type_from_array_type(idx),

            // Type operator (readonly, unique, etc.)
            k if k == syntax_kind_ext::TYPE_OPERATOR => self.get_type_from_type_operator(idx),

            // Function type (e.g., () => number, (x: string) => void)
            k if k == syntax_kind_ext::FUNCTION_TYPE => self.get_type_from_function_type(idx),

            // Type literal ({ a: number; b(): string; })
            k if k == syntax_kind_ext::TYPE_LITERAL => self.get_type_from_type_literal(idx),

            // Type query (typeof X) - returns the type of X
            k if k == syntax_kind_ext::TYPE_QUERY => self.get_type_from_type_query(idx),

            // Qualified name (A.B.C) - resolve namespace member access
            k if k == syntax_kind_ext::QUALIFIED_NAME => self.resolve_qualified_name(idx),

            // Default case - unknown node kind is an error
            _ => TypeId::ERROR,
        }
    }

    /// Get type from a type reference node (e.g., "number", "string", "MyType").
    fn get_type_from_type_reference(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // Get the TypeRefData from the arena
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type ref data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .map_or(false, |args| !args.nodes.is_empty());

        // Check if type_name is a qualified name (A.B)
        if let Some(name_node) = self.ctx.arena.get(type_name_idx) {
            if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                if has_type_args {
                    let Some(sym_id) = self.resolve_qualified_symbol(type_name_idx) else {
                        let _ = self.resolve_qualified_name(type_name_idx);
                        return TypeId::ERROR;
                    };
                    if self.alias_resolves_to_value_only(sym_id)
                        || self.symbol_is_value_only(sym_id)
                    {
                        let name = self
                            .entity_name_text(type_name_idx)
                            .unwrap_or_else(|| "<unknown>".to_string());
                        self.error_value_only_type_at(&name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if let Some(args) = &type_ref.type_arguments {
                        if self.should_resolve_recursive_type_alias(sym_id, args) {
                            // Ensure the base type symbol is resolved first so its type params
                            // are available in the type_env for Application expansion
                            let _ = self.get_type_of_symbol(sym_id);
                        }
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = crate::solver::TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    return lowering.lower_type(idx);
                }
                return self.resolve_qualified_name(type_name_idx);
            }
        }

        // Get the identifier for the type name
        if let Some(name_node) = self.ctx.arena.get(type_name_idx) {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let name = ident.escaped_text.as_str();

                if has_type_args {
                    let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                    let type_param = self.lookup_type_parameter(name);
                    let sym_id = self.resolve_identifier_symbol(type_name_idx);
                    if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                        // Try resolving from lib binders before falling back to UNKNOWN
                        if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                            // Still process type arguments for validation
                            if let Some(args) = &type_ref.type_arguments {
                                for &arg_idx in &args.nodes {
                                    let _ = self.get_type_from_type_node(arg_idx);
                                }
                            }
                            return type_id;
                        }
                        if self.is_known_global_type_name(name) {
                            if let Some(args) = &type_ref.type_arguments {
                                for &arg_idx in &args.nodes {
                                    let _ = self.get_type_from_type_node(arg_idx);
                                }
                            }
                            return TypeId::UNKNOWN;
                        }
                        if name == "await" {
                            self.error_cannot_find_name_did_you_mean_at(
                                name,
                                "Awaited",
                                type_name_idx,
                            );
                            return TypeId::ERROR;
                        }
                        self.error_cannot_find_name_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if !is_builtin_array {
                        if let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx) {
                            if self.alias_resolves_to_value_only(sym_id)
                                || self.symbol_is_value_only(sym_id)
                            {
                                self.error_value_only_type_at(name, type_name_idx);
                                return TypeId::ERROR;
                            }
                            if let Some(args) = &type_ref.type_arguments {
                                if self.should_resolve_recursive_type_alias(sym_id, args) {
                                    // Ensure the base type symbol is resolved first so its type params
                                    // are available in the type_env for Application expansion
                                    let _ = self.get_type_of_symbol(sym_id);
                                }
                            }
                        }
                    }
                    // Also ensure type arguments are resolved and in type_env
                    // This is needed so that when we evaluate the Application, we can
                    // resolve Ref types in the arguments
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            // Recursively get type from the arg - this will add any referenced
                            // symbols to type_env
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = crate::solver::TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    return lowering.lower_type(idx);
                }

                if name == "Array" || name == "ReadonlyArray" {
                    if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
                        return type_id;
                    }
                    let elem_type = type_ref
                        .type_arguments
                        .as_ref()
                        .and_then(|args| args.nodes.first().copied())
                        .map(|idx| self.get_type_from_type_node(idx))
                        .unwrap_or(TypeId::UNKNOWN);
                    let array_type = self.ctx.types.array(elem_type);
                    if name == "ReadonlyArray" {
                        return self
                            .ctx
                            .types
                            .intern(crate::solver::TypeKey::ReadonlyType(array_type));
                    }
                    return array_type;
                }

                // Check for built-in types (primitive keywords)
                match name {
                    "number" => return TypeId::NUMBER,
                    "string" => return TypeId::STRING,
                    "boolean" => return TypeId::BOOLEAN,
                    "void" => return TypeId::VOID,
                    "any" => return TypeId::ANY,
                    "never" => return TypeId::NEVER,
                    "unknown" => return TypeId::UNKNOWN,
                    "undefined" => return TypeId::UNDEFINED,
                    "null" => return TypeId::NULL,
                    "object" => return TypeId::OBJECT,
                    "bigint" => return TypeId::BIGINT,
                    "symbol" => return TypeId::SYMBOL,
                    _ => {}
                }

                // Check if this is a type parameter (generic type like T in function<T>)
                if let Some(type_param) = self.lookup_type_parameter(name) {
                    return type_param;
                }

                if name != "Array" && name != "ReadonlyArray" {
                    if let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx) {
                        if self.alias_resolves_to_value_only(sym_id)
                            || self.symbol_is_value_only(sym_id)
                        {
                            self.error_value_only_type_at(name, type_name_idx);
                            return TypeId::ERROR;
                        }
                    }
                }

                if let Some(type_id) = self.resolve_named_type_reference(name, type_name_idx) {
                    return type_id;
                }
                if name == "await" {
                    self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                    return TypeId::ERROR;
                }
                if self.is_known_global_type_name(name) {
                    return TypeId::UNKNOWN;
                }
                self.error_cannot_find_name_at(name, type_name_idx);
                return TypeId::ERROR;
            }
        }

        // Unknown type name node kind - propagate error
        TypeId::ERROR
    }

    fn should_resolve_recursive_type_alias(
        &self,
        sym_id: SymbolId,
        type_args: &crate::parser::NodeList,
    ) -> bool {
        if !self.ctx.symbol_resolution_set.contains(&sym_id) {
            return true;
        }
        if self.ctx.symbol_resolution_stack.last().copied() != Some(sym_id) {
            return true;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return true;
        }
        self.type_args_match_alias_params(sym_id, type_args)
    }

    fn type_args_match_alias_params(
        &self,
        sym_id: SymbolId,
        type_args: &crate::parser::NodeList,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };
        let Some(type_params) = &type_alias.type_parameters else {
            return false;
        };
        if type_params.nodes.len() != type_args.nodes.len() {
            return false;
        }

        for (&param_idx, &arg_idx) in type_params.nodes.iter().zip(type_args.nodes.iter()) {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(param_name) = self
                .ctx
                .arena
                .get(param.name)
                .and_then(|node| self.ctx.arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };

            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if arg_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let Some(arg_ref) = self.ctx.arena.get_type_ref(arg_node) else {
                    return false;
                };
                if arg_ref
                    .type_arguments
                    .as_ref()
                    .map_or(false, |list| !list.nodes.is_empty())
                {
                    return false;
                }
                let Some(arg_name_node) = self.ctx.arena.get(arg_ref.type_name) else {
                    return false;
                };
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_name_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else if arg_node.kind == SyntaxKind::Identifier as u16 {
                let Some(arg_ident) = self.ctx.arena.get_identifier(arg_node) else {
                    return false;
                };
                if arg_ident.escaped_text != param_name {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    fn class_instance_type_from_symbol(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        self.class_instance_type_with_params_from_symbol(sym_id)
            .map(|(instance_type, _)| instance_type)
    }

    fn class_instance_type_with_params_from_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<crate::solver::TypeParamInfo>)> {
        use crate::solver::{SymbolRef, TypeKey};

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        if !self.ctx.class_instance_resolution_set.insert(sym_id) {
            let fallback = self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0)));
            return Some((fallback, Vec::new()));
        }

        let (params, updates) = self.push_type_parameters(&class.type_parameters);
        let instance_type = self.get_class_instance_type(decl_idx, class);
        self.pop_type_parameters(updates);
        self.ctx.class_instance_resolution_set.remove(&sym_id);
        Some((instance_type, params))
    }

    fn type_reference_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        use crate::solver::TypeLowering;

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            // For merged class+namespace symbols, return the constructor type (with namespace exports)
            // instead of the instance type. This allows accessing namespace members via Foo.Bar.
            if symbol.flags & symbol_flags::CLASS != 0
                && symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) == 0
            {
                if let Some(instance_type) = self.class_instance_type_from_symbol(sym_id) {
                    return instance_type;
                }
            }
            if symbol.flags & symbol_flags::INTERFACE != 0 {
                if !symbol.declarations.is_empty() {
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type =
                        lowering.lower_interface_declarations(&symbol.declarations);
                    return self
                        .merge_interface_heritage_types(&symbol.declarations, interface_type);
                }
                if !symbol.value_declaration.is_none() {
                    return self.get_type_of_interface(symbol.value_declaration);
                }
            }
        }
        self.get_type_of_symbol(sym_id)
    }

    fn merge_namespace_exports_into_constructor(
        &mut self,
        sym_id: SymbolId,
        ctor_type: TypeId,
    ) -> TypeId {
        use crate::solver::{CallableShape, PropertyInfo, TypeKey};
        use rustc_hash::FxHashMap;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return ctor_type;
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return ctor_type;
        };
        let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(ctor_type) else {
            return ctor_type;
        };
        let shape = self.ctx.types.callable_shape(shape_id);

        let mut props: FxHashMap<Atom, PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();

        // Merge ALL exports from the namespace into the constructor type.
        // This includes both value exports (consts, functions) and type-only exports (interfaces, type aliases).
        // For merged class+namespace symbols, TypeScript allows accessing both value and type members.
        for (name, member_id) in exports.iter() {
            let type_id = self.get_type_of_symbol(*member_id);
            let name_atom = self.ctx.types.intern_string(name);
            props.entry(name_atom).or_insert(PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: false,
                readonly: false,
                is_method: false,
            });
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        self.ctx.types.callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: shape.construct_signatures.clone(),
            properties,
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        })
    }

    fn resolve_named_type_reference(&mut self, name: &str, name_idx: NodeIndex) -> Option<TypeId> {
        if let Some(type_id) = self.lookup_type_parameter(name) {
            return Some(type_id);
        }
        // Check if this is a global augmentation (interface declared in `declare global` block)
        // If so, use resolve_lib_type_by_name to merge with lib.d.ts declarations
        let is_global_augmentation = self.ctx.binder.global_augmentations.contains_key(name);
        if is_global_augmentation {
            // For global augmentations, we must use resolve_lib_type_by_name to get
            // the proper merge of lib.d.ts + user augmentation
            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                return Some(type_id);
            }
        }
        if let Some(sym_id) = self.resolve_identifier_symbol(name_idx) {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Fall back to lib contexts for global type resolution
        if let Some(type_id) = self.resolve_lib_type_by_name(name) {
            return Some(type_id);
        }
        None
    }

    /// Resolve a type by name from lib file contexts.
    /// This is used for global types like Object, Array, Promise, etc. from lib.d.ts.
    /// Also merges in any global augmentations from the current file.
    fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        use crate::solver::TypeLowering;

        let mut lib_type_id: Option<TypeId> = None;

        for lib_ctx in &self.ctx.lib_contexts {
            // Look up the symbol in this lib file's file_locals
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                // Get the symbol's declaration(s)
                if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) {
                    // Lower the type from the lib file's arena
                    let lowering = TypeLowering::new(lib_ctx.arena.as_ref(), self.ctx.types);
                    // For interfaces, use all declarations (handles declaration merging)
                    if !symbol.declarations.is_empty() {
                        lib_type_id = Some(lowering.lower_interface_declarations(&symbol.declarations));
                        break;
                    }
                    // For type aliases and other single-declaration types
                    let decl_idx = symbol.value_declaration;
                    if decl_idx.0 != u32::MAX {
                        lib_type_id = Some(lowering.lower_type(decl_idx));
                        break;
                    }
                }
            }
        }

        // Check for global augmentations in the current file that should merge with this type
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name) {
            if !augmentation_decls.is_empty() {
                // Lower the augmentation declarations from the current file's arena
                let lowering = TypeLowering::new(self.ctx.arena, self.ctx.types);
                let augmentation_type = lowering.lower_interface_declarations(augmentation_decls);

                // Merge lib type with augmentation using intersection
                if let Some(lib_type) = lib_type_id {
                    return Some(self.ctx.types.intersection2(lib_type, augmentation_type));
                } else {
                    // No lib type found, just return the augmentation
                    return Some(augmentation_type);
                }
            }
        }

        lib_type_id
    }

    fn lookup_type_parameter(&self, name: &str) -> Option<TypeId> {
        self.ctx.type_parameter_scope.get(name).copied()
    }

    /// Get all type parameter bindings for passing to TypeLowering.
    fn get_type_param_bindings(&self) -> Vec<(crate::interner::Atom, TypeId)> {
        self.ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect()
    }

    /// Resolve a qualified name or identifier to a symbol ID.
    fn resolve_qualified_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let mut visited_aliases = Vec::new();
        self.resolve_qualified_symbol_inner(idx, &mut visited_aliases)
    }

    fn resolve_qualified_symbol_inner(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(idx)?;
            return self.resolve_alias_symbol(sym_id, visited_aliases);
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let literal = self.ctx.arena.get_literal(node)?;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&literal.text) {
                return self.resolve_alias_symbol(sym_id, visited_aliases);
            }
            return None;
        }

        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qn = self.ctx.arena.get_qualified_name(node)?;
        let left_sym = self.resolve_qualified_symbol_inner(qn.left, visited_aliases)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited_aliases)?;
        let right_name = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        let member_sym = exports.get(right_name)?;
        self.resolve_alias_symbol(member_sym, visited_aliases)
    }

    fn resolve_require_call_symbol(
        &self,
        idx: NodeIndex,
        visited_aliases: Option<&mut Vec<SymbolId>>,
    ) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_node = self.ctx.arena.get(call.expression)?;
        let callee_ident = self.ctx.arena.get_identifier(callee_node)?;
        if callee_ident.escaped_text != "require" {
            return None;
        }

        let args = call.arguments.as_ref()?;
        let first_arg = args.nodes.first().copied()?;
        let arg_node = self.ctx.arena.get(first_arg)?;
        let literal = self.ctx.arena.get_literal(arg_node)?;
        let sym_id = self.ctx.binder.file_locals.get(&literal.text)?;

        if let Some(visited) = visited_aliases {
            return self.resolve_alias_symbol(sym_id, visited);
        }
        Some(sym_id)
    }

    /// Check if a node is a `require()` call expression.
    /// This is used to detect import equals declarations like `import x = require('./module')`
    /// where we want to return ANY type instead of the literal string type.
    fn is_require_call(&self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let call = match self.ctx.arena.get_call_expr(node) {
            Some(c) => c,
            None => return false,
        };

        let callee_node = match self.ctx.arena.get(call.expression) {
            Some(n) => n,
            None => return false,
        };

        let callee_ident = match self.ctx.arena.get_identifier(callee_node) {
            Some(ident) => ident,
            None => return false,
        };

        callee_ident.escaped_text == "require"
    }

    fn missing_type_query_left(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                if self.resolve_identifier_symbol(current).is_none() {
                    return Some(current);
                }
                return None;
            }
            if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                return None;
            }
            let qn = self.ctx.arena.get_qualified_name(node)?;
            current = qn.left;
        }
    }

    fn report_type_query_missing_member(&mut self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return false,
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return false,
        };

        let left_sym = match self.resolve_qualified_symbol(qn.left) {
            Some(sym) => sym,
            None => return false,
        };
        let left_symbol = match self.ctx.binder.get_symbol(left_sym) {
            Some(symbol) => symbol,
            None => return false,
        };
        let exports = match left_symbol.exports.as_ref() {
            Some(exports) => exports,
            None => return false,
        };
        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.clone())
        {
            Some(name) => name,
            None => return false,
        };

        if exports.has(&right_name) {
            return false;
        }

        let namespace_name = self
            .entity_name_text(qn.left)
            .unwrap_or_else(|| left_symbol.escaped_name.clone());
        self.error_namespace_no_export(&namespace_name, &right_name, qn.right);
        true
    }

    fn resolve_alias_symbol(
        &self,
        sym_id: SymbolId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited_aliases.iter().any(|&seen| seen == sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            let import = self.ctx.arena.get_import_decl(decl_node)?;
            if let Some(target) =
                self.resolve_qualified_symbol_inner(import.module_specifier, visited_aliases)
            {
                return Some(target);
            }
            return self
                .resolve_require_call_symbol(import.module_specifier, Some(visited_aliases));
        }
        None
    }

    fn entity_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let left = self.entity_name_text(qn.left)?;
            let right = self.entity_name_text(qn.right)?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }
        None
    }

    fn resolve_heritage_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.resolve_qualified_symbol(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
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
            return exports.get(&name);
        }

        None
    }

    fn heritage_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.entity_name_text(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left = self.heritage_name_text(access.expression)?;
            let right = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.clone())?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }

        // Handle keyword literals in heritage clauses (e.g., extends null, extends true)
        match node.kind {
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => return Some("undefined".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => return Some("0".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => return Some("0".to_string()),
            _ => {}
        }

        None
    }

    fn apply_type_arguments_to_constructor_type(
        &mut self,
        ctor_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use crate::solver::{CallableShape, TypeKey};

        let Some(type_arguments) = type_arguments else {
            return ctor_type;
        };

        if type_arguments.nodes.is_empty() {
            return ctor_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return ctor_type;
        }

        let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(ctor_type) else {
            return ctor_type;
        };

        let shape = self.ctx.types.callable_shape(shape_id);
        let mut matching: Vec<&crate::solver::CallSignature> = shape
            .construct_signatures
            .iter()
            .filter(|sig| sig.type_params.len() == type_args.len())
            .collect();

        if matching.is_empty() {
            matching = shape
                .construct_signatures
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .collect();
        }

        if matching.is_empty() {
            return ctor_type;
        }

        let instantiated_constructs: Vec<crate::solver::CallSignature> = matching
            .iter()
            .map(|sig| {
                let mut args = type_args.clone();
                if args.len() < sig.type_params.len() {
                    for param in sig.type_params.iter().skip(args.len()) {
                        let fallback = param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                        args.push(fallback);
                    }
                }
                if args.len() > sig.type_params.len() {
                    args.truncate(sig.type_params.len());
                }
                self.instantiate_constructor_signature(sig, &args)
            })
            .collect();

        let new_shape = CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: instantiated_constructs,
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        };
        self.ctx.types.callable(new_shape)
    }

    fn instantiate_constructor_signature(
        &self,
        sig: &crate::solver::CallSignature,
        type_args: &[TypeId],
    ) -> crate::solver::CallSignature {
        use crate::solver::{
            CallSignature, ParamInfo, TypePredicate, TypeSubstitution, instantiate_type,
        };

        let substitution = TypeSubstitution::from_args(&sig.type_params, type_args);
        let params: Vec<ParamInfo> = sig
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name.clone(),
                type_id: instantiate_type(self.ctx.types, param.type_id, &substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();

        let this_type = sig
            .this_type
            .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution));
        let return_type = instantiate_type(self.ctx.types, sig.return_type, &substitution);
        let type_predicate = sig.type_predicate.as_ref().map(|predicate| TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target.clone(),
            type_id: predicate
                .type_id
                .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution)),
        });

        CallSignature {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
        }
    }

    fn base_constructor_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if let Some(name) = self.heritage_name_text(expr_idx) {
            // Filter out primitive types and literals that cannot be used in class extends
            if matches!(
                name.as_str(),
                "null" | "undefined" | "true" | "false" | "void" | "0"
                    | "number" | "string" | "boolean"
                    | "never" | "unknown" | "any"
            ) {
                return None;
            }
        }
        let expr_type = self.get_type_of_node(expr_idx);

        // Evaluate application types to get the actual intersection type
        let evaluated_type = self.evaluate_application_type(expr_type);

        let ctor_types = self.constructor_types_from_type(evaluated_type);
        if ctor_types.is_empty() {
            return None;
        }
        let ctor_type = if ctor_types.len() == 1 {
            ctor_types[0]
        } else {
            self.ctx.types.intersection(ctor_types)
        };
        Some(self.apply_type_arguments_to_constructor_type(ctor_type, type_arguments))
    }

    fn constructor_types_from_type(&mut self, type_id: TypeId) -> Vec<TypeId> {
        use rustc_hash::FxHashSet;

        self.ensure_application_symbols_resolved(type_id);
        let mut ctor_types = Vec::new();
        let mut visited = FxHashSet::default();
        self.collect_constructor_types_from_type_inner(type_id, &mut ctor_types, &mut visited);
        ctor_types
    }

    fn collect_constructor_types_from_type_inner(
        &mut self,
        type_id: TypeId,
        ctor_types: &mut Vec<TypeId>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::TypeKey;

        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        if !visited.insert(evaluated) {
            return;
        }

        let Some(key) = self.ctx.types.lookup(evaluated) else {
            return;
        };

        match key {
            TypeKey::Callable(_) => {
                ctor_types.push(evaluated);
            }
            TypeKey::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.is_constructor {
                    ctor_types.push(evaluated);
                }
            }
            TypeKey::Intersection(members_id) | TypeKey::Union(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                for &member in members.iter() {
                    self.collect_constructor_types_from_type_inner(member, ctor_types, visited);
                }
            }
            TypeKey::ReadonlyType(inner) => {
                self.collect_constructor_types_from_type_inner(inner, ctor_types, visited);
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.collect_constructor_types_from_type_inner(constraint, ctor_types, visited);
                }
            }
            TypeKey::Conditional(_)
            | TypeKey::Mapped(_)
            | TypeKey::IndexAccess(_, _)
            | TypeKey::KeyOf(_) => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            TypeKey::Application(_) => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            TypeKey::TypeQuery(sym_ref) => {
                // typeof X - get the type of the symbol X and collect constructors from it
                use crate::binder::SymbolId;
                let sym_id = SymbolId(sym_ref.0);
                let sym_type = self.get_type_of_symbol(sym_id);
                self.collect_constructor_types_from_type_inner(sym_type, ctor_types, visited);
            }
            _ => {}
        }
    }

    fn static_properties_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo> {
        use rustc_hash::{FxHashMap, FxHashSet};

        self.ensure_application_symbols_resolved(type_id);
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        self.collect_static_properties_from_type_inner(type_id, &mut props, &mut visited);
        props
    }

    fn collect_static_properties_from_type_inner(
        &mut self,
        type_id: TypeId,
        props: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::TypeKey;

        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        if !visited.insert(evaluated) {
            return;
        }

        let Some(key) = self.ctx.types.lookup(evaluated) else {
            return;
        };

        match key {
            TypeKey::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                for prop in shape.properties.iter() {
                    props.entry(prop.name).or_insert_with(|| prop.clone());
                }
            }
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    props.entry(prop.name).or_insert_with(|| prop.clone());
                }
            }
            TypeKey::Intersection(members_id) | TypeKey::Union(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                for &member in members.iter() {
                    self.collect_static_properties_from_type_inner(member, props, visited);
                }
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.collect_static_properties_from_type_inner(constraint, props, visited);
                }
            }
            TypeKey::ReadonlyType(inner) => {
                self.collect_static_properties_from_type_inner(inner, props, visited);
            }
            TypeKey::Conditional(_)
            | TypeKey::Mapped(_)
            | TypeKey::IndexAccess(_, _)
            | TypeKey::KeyOf(_) => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            TypeKey::Application(_) => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            _ => {}
        }
    }

    fn base_instance_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        let ctor_type = self.base_constructor_type_from_expression(expr_idx, type_arguments)?;
        self.instance_type_from_constructor_type(ctor_type)
    }

    fn merge_constructor_properties_from_type(
        &mut self,
        ctor_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
    ) {
        let base_props = self.static_properties_from_type(ctor_type);
        for (name, prop) in base_props {
            properties.entry(name).or_insert(prop);
        }
    }

    fn merge_base_instance_properties(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        string_index: &mut Option<crate::solver::IndexSignature>,
        number_index: &mut Option<crate::solver::IndexSignature>,
    ) {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.merge_base_instance_properties_inner(
            base_instance_type,
            properties,
            string_index,
            number_index,
            &mut visited,
        );
    }

    fn merge_base_instance_properties_inner(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
        string_index: &mut Option<crate::solver::IndexSignature>,
        number_index: &mut Option<crate::solver::IndexSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        use crate::solver::TypeKey;

        if !visited.insert(base_instance_type) {
            return;
        }

        match self.ctx.types.lookup(base_instance_type) {
            Some(TypeKey::Object(base_shape_id) | TypeKey::ObjectWithIndex(base_shape_id)) => {
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                for base_prop in base_shape.properties.iter() {
                    properties
                        .entry(base_prop.name)
                        .or_insert_with(|| base_prop.clone());
                }
                if let Some(ref idx) = base_shape.string_index {
                    Self::merge_index_signature(string_index, idx.clone());
                }
                if let Some(ref idx) = base_shape.number_index {
                    Self::merge_index_signature(number_index, idx.clone());
                }
            }
            Some(TypeKey::Intersection(members_id)) => {
                let members = self.ctx.types.type_list(members_id);
                for &member in members.iter() {
                    self.merge_base_instance_properties_inner(
                        member,
                        properties,
                        string_index,
                        number_index,
                        visited,
                    );
                }
            }
            Some(TypeKey::Union(members_id)) => {
                use rustc_hash::FxHashMap;
                let members = self.ctx.types.type_list(members_id);
                let mut common_props: Option<FxHashMap<Atom, crate::solver::PropertyInfo>> = None;
                let mut common_string_index: Option<crate::solver::IndexSignature> = None;
                let mut common_number_index: Option<crate::solver::IndexSignature> = None;

                for &member in members.iter() {
                    let mut member_props: FxHashMap<Atom, crate::solver::PropertyInfo> =
                        FxHashMap::default();
                    let mut member_string_index = None;
                    let mut member_number_index = None;
                    let mut member_visited = rustc_hash::FxHashSet::default();
                    member_visited.insert(base_instance_type);

                    self.merge_base_instance_properties_inner(
                        member,
                        &mut member_props,
                        &mut member_string_index,
                        &mut member_number_index,
                        &mut member_visited,
                    );

                    if common_props.is_none() {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        continue;
                    }

                    let mut props = common_props.take().unwrap();
                    props.retain(|name, prop| {
                        let Some(member_prop) = member_props.get(name) else {
                            return false;
                        };
                        let merged_type = if prop.type_id == member_prop.type_id {
                            prop.type_id
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.type_id, member_prop.type_id])
                        };
                        let merged_write_type = if prop.write_type == member_prop.write_type {
                            prop.write_type
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.write_type, member_prop.write_type])
                        };
                        prop.type_id = merged_type;
                        prop.write_type = merged_write_type;
                        prop.optional |= member_prop.optional;
                        prop.readonly &= member_prop.readonly;
                        prop.is_method &= member_prop.is_method;
                        true
                    });
                    common_props = Some(props);

                    common_string_index = match (common_string_index.take(), member_string_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };
                    common_number_index = match (common_number_index.take(), member_number_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };

                    if common_props.as_ref().map_or(true, |props| props.is_empty())
                        && common_string_index.is_none()
                        && common_number_index.is_none()
                    {
                        break;
                    }
                }

                if let Some(props) = common_props {
                    for prop in props.into_values() {
                        properties.entry(prop.name).or_insert(prop);
                    }
                }
                if let Some(idx) = common_string_index {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = common_number_index {
                    Self::merge_index_signature(number_index, idx);
                }
            }
            _ => {}
        }
    }

    fn resolve_type_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        let sym_id = self.resolve_qualified_symbol(idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::TYPE) != 0 {
            Some(sym_id.0)
        } else {
            None
        }
    }

    fn resolve_value_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        let sym_id = self.resolve_qualified_symbol(idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
            Some(sym_id.0)
        } else {
            None
        }
    }

    /// Resolve a qualified name (A.B) to a type.
    /// Returns the type of the rightmost member, or reports TS2694 if not found.
    fn resolve_qualified_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
            return TypeId::ERROR; // Missing qualified name data - propagate error
        };

        // Resolve the left side (could be Identifier or another QualifiedName)
        let left_type = if let Some(left_node) = self.ctx.arena.get(qn.left) {
            if left_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                self.resolve_qualified_name(qn.left)
            } else if left_node.kind == SyntaxKind::Identifier as u16 {
                // Resolve identifier as a type reference
                self.get_type_from_type_reference_by_name(qn.left)
            } else {
                TypeId::ERROR // Unknown node kind - propagate error
            }
        } else {
            TypeId::ERROR // Missing left node - propagate error
        };

        if left_type == TypeId::ANY || left_type == TypeId::ERROR {
            return TypeId::ERROR; // Propagate error from left side
        }

        // Get the right side name (B in A.B)
        let right_name = if let Some(right_node) = self.ctx.arena.get(qn.right) {
            if let Some(id) = self.ctx.arena.get_identifier(right_node) {
                id.escaped_text.clone()
            } else {
                return TypeId::ERROR; // Missing identifier data - propagate error
            }
        } else {
            return TypeId::ERROR; // Missing right node - propagate error
        };

        // First, try to resolve the left side as a symbol and check its exports.
        // This handles merged class+namespace, function+namespace, and enum+namespace symbols.
        let mut member_sym_id_from_symbol = None;
        if let Some(left_node) = self.ctx.arena.get(qn.left) {
            if left_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.resolve_identifier_symbol(qn.left) {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        if let Some(ref exports) = symbol.exports {
                            if let Some(member_id) = exports.get(&right_name) {
                                member_sym_id_from_symbol = Some(member_id);
                            }
                        }
                    }
                }
            }
        }

        // If found via symbol resolution, use it
        if let Some(member_sym_id) = member_sym_id_from_symbol {
            if let Some(member_symbol) = self.ctx.binder.get_symbol(member_sym_id) {
                let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                if !is_namespace {
                    if self.alias_resolves_to_value_only(member_sym_id)
                        || self.symbol_is_value_only(member_sym_id)
                    {
                        self.error_value_only_type_at(&right_name, qn.right);
                        return TypeId::ERROR;
                    }
                }
            }
            return self.type_reference_symbol_type(member_sym_id);
        }

        // Otherwise, fall back to type-based lookup for pure namespace/module types
        // Look up the member in the left side's exports
        if let Some(crate::solver::TypeKey::Ref(crate::solver::SymbolRef(sym_id))) =
            self.ctx.types.lookup(left_type)
        {
            if let Some(symbol) = self.ctx.binder.get_symbol(crate::binder::SymbolId(sym_id)) {
                // Check exports table
                if let Some(ref exports) = symbol.exports {
                    if let Some(member_sym_id) = exports.get(&right_name) {
                        // Check value-only, but skip for namespaces since they can be used
                        // to navigate to types (e.g., Outer.Inner.Type)
                        if let Some(member_symbol) = self.ctx.binder.get_symbol(member_sym_id) {
                            let is_namespace = member_symbol.flags & symbol_flags::MODULE != 0;
                            if !is_namespace {
                                if self.alias_resolves_to_value_only(member_sym_id)
                                    || self.symbol_is_value_only(member_sym_id)
                                {
                                    self.error_value_only_type_at(&right_name, qn.right);
                                    return TypeId::ERROR;
                                }
                            }
                        }
                        return self.type_reference_symbol_type(member_sym_id);
                    }
                }

                // Not found - report TS2694
                let namespace_name = self
                    .entity_name_text(qn.left)
                    .unwrap_or_else(|| symbol.escaped_name.clone());
                self.error_namespace_no_export(&namespace_name, &right_name, qn.right);
                return TypeId::ERROR;
            }
        }

        // Left side wasn't a reference to a namespace/module
        TypeId::ANY
    }

    /// Helper to resolve an identifier as a type reference (for qualified name left sides).
    fn get_type_from_type_reference_by_name(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            let name = &ident.escaped_text;

            if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                return self.type_reference_symbol_type(sym_id);
            }

            // Not found
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }

        TypeId::ERROR // Not an identifier - propagate error
    }

    /// Get type from a union type node (A | B).
    fn get_type_from_union_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // UnionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Use get_type_from_type_node to properly resolve typeof expressions via binder
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::NEVER;
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return self.ctx.types.union(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from an intersection type node (A & B).
    fn get_type_from_intersection_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        // IntersectionType uses CompositeTypeData which has a types list
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            let mut member_types = Vec::new();
            for &type_idx in &composite.types.nodes {
                // Use get_type_from_type_node to properly resolve typeof expressions via binder
                member_types.push(self.get_type_from_type_node(type_idx));
            }

            if member_types.is_empty() {
                return TypeId::UNKNOWN; // Empty intersection is unknown
            }
            if member_types.len() == 1 {
                return member_types[0];
            }

            return self.ctx.types.intersection(member_types);
        }

        TypeId::ERROR // Missing composite type data - propagate error
    }

    /// Get type from a type query node (typeof X).
    /// Creates a TypeQuery type with the actual SymbolId from the binder.
    fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR; // Missing type query data - propagate error
        };

        let name_text = self.entity_name_text(type_query.expr_name);
        let is_identifier = self
            .ctx
            .arena
            .get(type_query.expr_name)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();
        let has_type_args = type_query
            .type_arguments
            .as_ref()
            .map_or(false, |args| !args.nodes.is_empty());

        let base =
            if let Some(sym_id) = self.resolve_value_symbol_for_lowering(type_query.expr_name) {
                eprintln!("=== get_type_from_type_query ===");
                eprintln!("  name: {:?}, sym_id: {:?}", name_text, sym_id);
                if !has_type_args {
                    let resolved = self.get_type_of_symbol(crate::binder::SymbolId(sym_id));
                    eprintln!("  resolved type: {:?}", resolved);
                    if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                        eprintln!("  => returning resolved type directly");
                        return resolved;
                    }
                }
                let typequery_type = self.ctx.types.intern(TypeKey::TypeQuery(SymbolRef(sym_id)));
                eprintln!("  => returning TypeQuery type: {:?}", typequery_type);
                typequery_type
            } else if self
                .resolve_type_symbol_for_lowering(type_query.expr_name)
                .is_some()
            {
                let name = name_text.as_deref().unwrap_or("<unknown>");
                self.error_type_only_value_at(name, type_query.expr_name);
                return TypeId::ERROR;
            } else if let Some(name) = name_text {
                if is_identifier {
                    if self.is_known_global_value_name(&name) {
                        return TypeId::ANY; // Known global but not resolved - use ANY to allow property access
                    }
                    self.error_cannot_find_name_at(&name, type_query.expr_name);
                    return TypeId::ERROR;
                }
                if let Some(missing_idx) = self.missing_type_query_left(type_query.expr_name) {
                    if let Some(missing_name) = self
                        .ctx
                        .arena
                        .get(missing_idx)
                        .and_then(|node| self.ctx.arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.clone())
                    {
                        self.error_cannot_find_name_at(&missing_name, missing_idx);
                        return TypeId::ERROR;
                    }
                }
                if self.report_type_query_missing_member(type_query.expr_name) {
                    return TypeId::ERROR;
                }
                // Not found - fall back to hash (for forward compatibility)
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                name.hash(&mut hasher);
                let symbol_id = hasher.finish() as u32;
                self.ctx
                    .types
                    .intern(TypeKey::TypeQuery(SymbolRef(symbol_id)))
            } else {
                return TypeId::ERROR; // No name text - propagate error
            };

        if let Some(args) = &type_query.type_arguments {
            if !args.nodes.is_empty() {
                let type_args = args
                    .nodes
                    .iter()
                    .map(|&idx| self.get_type_from_type_node(idx))
                    .collect();
                return self.ctx.types.application(base, type_args);
            }
        }

        base
    }

    /// Get type from an array type node (T[]).
    fn get_type_from_array_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(array_type) = self.ctx.arena.get_array_type(node) {
            let elem_type = self.get_type_from_type_node(array_type.element_type);
            return self.ctx.types.array(elem_type);
        }

        TypeId::ERROR // Missing array type data - propagate error
    }

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.get_type_from_type_node(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                // Wrap the inner type in ReadonlyType
                return self.ctx.types.intern(crate::solver::TypeKey::ReadonlyType(inner_type));
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR // Missing type operator data - propagate error
        }
    }

    /// Get type from a function type node (e.g., () => number, (x: string) => void).
    fn get_type_from_function_type(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::TypeLowering;

        let type_param_bindings = self.get_type_param_bindings();
        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);

        lowering.lower_type(idx)
    }

    fn get_type_from_type_node_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self.get_type_from_type_reference_in_type_literal(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            return self.get_type_from_type_query(idx);
        }
        if node.kind == syntax_kind_ext::UNION_TYPE {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                let members = composite
                    .types
                    .nodes
                    .iter()
                    .map(|&member_idx| self.get_type_from_type_node_in_type_literal(member_idx))
                    .collect::<Vec<_>>();
                return self.ctx.types.union(members);
            }
            return TypeId::ERROR;
        }
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                let elem_type =
                    self.get_type_from_type_node_in_type_literal(array_type.element_type);
                return self.ctx.types.array(elem_type);
            }
            return TypeId::ERROR; // Missing array type data - propagate error
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR {
            // Handle readonly and other type operators in type literals
            return self.get_type_from_type_operator(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.get_type_from_type_literal(idx);
        }

        self.get_type_from_type_node(idx)
    }

    fn get_type_from_type_reference_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type reference data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .map_or(false, |args| !args.nodes.is_empty());

        if let Some(name_node) = self.ctx.arena.get(type_name_idx) {
            if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(sym_id) = self.resolve_qualified_symbol(type_name_idx) else {
                    let _ = self.resolve_qualified_name(type_name_idx);
                    return TypeId::ERROR;
                };
                if self.alias_resolves_to_value_only(sym_id) || self.symbol_is_value_only(sym_id) {
                    let name = self
                        .entity_name_text(type_name_idx)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    self.error_value_only_type_at(&name, type_name_idx);
                    return TypeId::ERROR;
                }
                let base_type = self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0)));
                if has_type_args {
                    let type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| {
                                    self.get_type_from_type_node_in_type_literal(arg_idx)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    return self.ctx.types.application(base_type, type_args);
                }
                return base_type;
            }
        }

        if let Some(name_node) = self.ctx.arena.get(type_name_idx) {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                let name = ident.escaped_text.as_str();

                if has_type_args {
                    let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                    let type_param = self.lookup_type_parameter(name);
                    let sym_id = self.resolve_identifier_symbol(type_name_idx);

                    if is_builtin_array && type_param.is_none() && sym_id.is_none() {
                        let elem_type = type_ref
                            .type_arguments
                            .as_ref()
                            .and_then(|args| args.nodes.first().copied())
                            .map(|idx| self.get_type_from_type_node_in_type_literal(idx))
                            .unwrap_or(TypeId::UNKNOWN);
                        let array_type = self.ctx.types.array(elem_type);
                        if name == "ReadonlyArray" {
                            return self.ctx.types.intern(TypeKey::ReadonlyType(array_type));
                        }
                        return array_type;
                    }

                    if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                        if self.is_known_global_type_name(name) {
                            if let Some(args) = &type_ref.type_arguments {
                                for &arg_idx in &args.nodes {
                                    let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                                }
                            }
                            return TypeId::UNKNOWN;
                        }
                        if name == "await" {
                            self.error_cannot_find_name_did_you_mean_at(
                                name,
                                "Awaited",
                                type_name_idx,
                            );
                            return TypeId::ERROR;
                        }
                        self.error_cannot_find_name_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if !is_builtin_array {
                        if let Some(sym_id) = sym_id {
                            if self.alias_resolves_to_value_only(sym_id)
                                || self.symbol_is_value_only(sym_id)
                            {
                                self.error_value_only_type_at(name, type_name_idx);
                                return TypeId::ERROR;
                            }
                        }
                    }

                    let base_type = if let Some(type_param) = type_param {
                        type_param
                    } else if let Some(sym_id) = sym_id {
                        self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0)))
                    } else {
                        TypeId::ERROR
                    };

                    let type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| {
                                    self.get_type_from_type_node_in_type_literal(arg_idx)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    return self.ctx.types.application(base_type, type_args);
                }

                if name == "Array" || name == "ReadonlyArray" {
                    if let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx) {
                        return self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0)));
                    }
                    if let Some(type_param) = self.lookup_type_parameter(name) {
                        return type_param;
                    }
                    let elem_type = type_ref
                        .type_arguments
                        .as_ref()
                        .and_then(|args| args.nodes.first().copied())
                        .map(|idx| self.get_type_from_type_node_in_type_literal(idx))
                        .unwrap_or(TypeId::UNKNOWN);
                    let array_type = self.ctx.types.array(elem_type);
                    if name == "ReadonlyArray" {
                        return self.ctx.types.intern(TypeKey::ReadonlyType(array_type));
                    }
                    return array_type;
                }

                match name {
                    "number" => return TypeId::NUMBER,
                    "string" => return TypeId::STRING,
                    "boolean" => return TypeId::BOOLEAN,
                    "void" => return TypeId::VOID,
                    "any" => return TypeId::ANY,
                    "never" => return TypeId::NEVER,
                    "unknown" => return TypeId::UNKNOWN,
                    "undefined" => return TypeId::UNDEFINED,
                    "null" => return TypeId::NULL,
                    "object" => return TypeId::OBJECT,
                    "bigint" => return TypeId::BIGINT,
                    "symbol" => return TypeId::SYMBOL,
                    _ => {}
                }

                if name != "Array" && name != "ReadonlyArray" {
                    if let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx) {
                        if self.alias_resolves_to_value_only(sym_id)
                            || self.symbol_is_value_only(sym_id)
                        {
                            self.error_value_only_type_at(name, type_name_idx);
                            return TypeId::ERROR;
                        }
                    }
                }

                if let Some(type_param) = self.lookup_type_parameter(name) {
                    return type_param;
                }
                if let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx) {
                    return self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0)));
                }

                if name == "await" {
                    self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                    return TypeId::ERROR;
                }
                if self.is_known_global_type_name(name) {
                    return TypeId::UNKNOWN;
                }
                self.error_cannot_find_name_at(name, type_name_idx);
                return TypeId::ERROR;
            }
        }

        TypeId::ANY
    }

    fn extract_params_from_signature_in_type_literal(
        &mut self,
        sig: &crate::parser::thin_node::SignatureData,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        self.extract_params_from_parameter_list_in_type_literal(params_list)
    }

    fn extract_params_from_parameter_list_in_type_literal(
        &mut self,
        params_list: &crate::parser::NodeList,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        use crate::solver::ParamInfo;

        let mut params = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let type_id = if !param.type_annotation.is_none() {
                self.get_type_from_type_node_in_type_literal(param.type_annotation)
            } else {
                TypeId::ANY
            };

            let name_node = self.ctx.arena.get(param.name);
            if let Some(name_node) = name_node {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    continue;
                }
            }

            let name: Option<Atom> = if let Some(name_node) = name_node {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    Some(self.ctx.types.intern_string(&name_data.escaped_text))
                } else {
                    None
                }
            } else {
                None
            };

            let optional = param.question_token || !param.initializer.is_none();
            let rest = param.dot_dot_dot_token;

            if let Some(name_atom) = name {
                if name_atom == this_atom {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    continue;
                }
            }

            params.push(ParamInfo {
                name,
                type_id,
                optional,
                rest,
            });
        }

        (params, this_type)
    }

    /// Get type from a type literal node ({ x: T }).
    fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use crate::solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectShape, PropertyInfo,
        };

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR; // Missing type literal data - propagate error
        };

        let mut properties = Vec::new();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(sig) = self.ctx.arena.get_signature(member) {
                match member.kind {
                    CALL_SIGNATURE => {
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) =
                            self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                        call_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) =
                            self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                        construct_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);

                        if member.kind == METHOD_SIGNATURE {
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, this_type) =
                                self.extract_params_from_signature_in_type_literal(sig);
                            let (return_type, type_predicate) =
                                self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                            let shape = FunctionShape {
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate,
                                is_constructor: false,
                                is_method: true,
                            };
                            let method_type = self.ctx.types.function(shape);
                            self.pop_type_parameters(type_param_updates);
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id: method_type,
                                write_type: method_type,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: true,
                            });
                        } else {
                            let type_id = if !sig.type_annotation.is_none() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: false,
                            });
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if !param_data.type_annotation.is_none() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    // Return UNKNOWN instead of ANY for index signature key without annotation
                    TypeId::UNKNOWN
                };
                let value_type = if !index_sig.type_annotation.is_none() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    // Return UNKNOWN instead of ANY for index signature value without annotation
                    TypeId::UNKNOWN
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                };
                if key_type == TypeId::NUMBER {
                    number_index = Some(info);
                } else {
                    string_index = Some(info);
                }
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            return self.ctx.types.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
            });
        }

        if string_index.is_some() || number_index.is_some() {
            return self.ctx.types.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
            });
        }

        self.ctx.types.object(properties)
    }

    fn lower_type_parameter_info(
        &mut self,
        idx: NodeIndex,
    ) -> Option<(crate::solver::TypeParamInfo, String)> {
        let node = self.ctx.arena.get(idx)?;
        let data = self.ctx.arena.get_type_parameter(node)?;

        let name = self
            .ctx
            .arena
            .get(data.name)
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|id_data| id_data.escaped_text.clone())
            .unwrap_or_else(|| "T".to_string());
        let atom = self.ctx.types.intern_string(&name);

        let constraint = if data.constraint != NodeIndex::NONE {
            Some(self.get_type_from_type_node(data.constraint))
        } else {
            None
        };

        let default = if data.default != NodeIndex::NONE {
            Some(self.get_type_from_type_node(data.default))
        } else {
            None
        };

        Some((
            crate::solver::TypeParamInfo {
                name: atom,
                constraint,
                default,
            },
            name,
        ))
    }

    fn push_type_parameters(
        &mut self,
        type_parameters: &Option<crate::parser::NodeList>,
    ) -> (
        Vec<crate::solver::TypeParamInfo>,
        Vec<(String, Option<TypeId>)>,
    ) {
        use crate::solver::TypeKey;

        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        let mut params = Vec::new();
        let mut updates = Vec::new();
        let mut param_indices = Vec::new();

        // First pass: Add all type parameters to scope WITHOUT resolving constraints
        // This allows self-referential constraints like T extends Box<T>
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_else(|| "T".to_string());
            let atom = self.ctx.types.intern_string(&name);

            // Create unconstrained type parameter initially
            let info = crate::solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
            };
            let type_id = self.ctx.types.intern(TypeKey::TypeParameter(info));
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
            param_indices.push(param_idx);
        }

        // Second pass: Now resolve constraints and defaults with all type parameters in scope
        for (idx, &param_idx) in param_indices.iter().enumerate() {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|id_data| id_data.escaped_text.clone())
                .unwrap_or_else(|| "T".to_string());
            let atom = self.ctx.types.intern_string(&name);

            let constraint = if data.constraint != NodeIndex::NONE {
                let constraint_type = self.get_type_from_type_node(data.constraint);
                // Validate that the constraint is a valid type
                if constraint_type == TypeId::ERROR {
                    // If constraint is invalid, emit diagnostic and use unknown
                    self.error_at_node(
                        data.constraint,
                        crate::checker::types::diagnostics::diagnostic_messages::TYPE_NOT_SATISFY_CONSTRAINT,
                        crate::checker::types::diagnostics::diagnostic_codes::CONSTRAINT_OF_TYPE_PARAMETER,
                    );
                    Some(TypeId::UNKNOWN)
                } else {
                    Some(constraint_type)
                }
            } else {
                None
            };

            let default = if data.default != NodeIndex::NONE {
                let default_type = self.get_type_from_type_node(data.default);
                // Validate that default satisfies constraint if present
                if let Some(constraint_type) = constraint {
                    if default_type != TypeId::ERROR && !self.is_assignable_to(default_type, constraint_type) {
                        self.error_at_node(
                            data.default,
                            crate::checker::types::diagnostics::diagnostic_messages::TYPE_NOT_SATISFY_CONSTRAINT,
                            crate::checker::types::diagnostics::diagnostic_codes::TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED,
                        );
                    }
                }
                if default_type == TypeId::ERROR {
                    None
                } else {
                    Some(default_type)
                }
            } else {
                None
            };

            let info = crate::solver::TypeParamInfo {
                name: atom,
                constraint,
                default,
            };
            params.push(info.clone());

            // UPDATE: Create a new TypeParameter with constraints and update the scope
            // This ensures that when function parameters reference these type parameters,
            // they get the constrained version, not the unconstrained placeholder
            let constrained_type_id = self.ctx.types.intern(TypeKey::TypeParameter(info));
            self.ctx
                .type_parameter_scope
                .insert(name.clone(), constrained_type_id);
        }

        (params, updates)
    }

    fn pop_type_parameters(&mut self, updates: Vec<(String, Option<TypeId>)>) {
        for (name, previous) in updates.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }

    /// Collect all `infer` type parameter names from a type node.
    /// This is used to add inferred type parameters to the scope when checking conditional types.
    fn collect_infer_type_parameters(&self, type_idx: NodeIndex) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_infer_type_parameters_inner(type_idx, &mut params);
        params
    }

    fn collect_infer_type_parameters_inner(&self, type_idx: NodeIndex, params: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    if let Some(param_node) = self.ctx.arena.get(infer.type_parameter) {
                        if let Some(param) = self.ctx.arena.get_type_parameter(param_node) {
                            if let Some(name_node) = self.ctx.arena.get(param.name) {
                                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                                    let name = ident.escaped_text.clone();
                                    if !params.contains(&name) {
                                        params.push(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    if let Some(ref args) = type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            self.collect_infer_type_parameters_inner(arg_idx, params);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.collect_infer_type_parameters_inner(member_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    for &param_idx in &func_type.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                if !param.type_annotation.is_none() {
                                    self.collect_infer_type_parameters_inner(
                                        param.type_annotation,
                                        params,
                                    );
                                }
                            }
                        }
                    }
                    if !func_type.type_annotation.is_none() {
                        self.collect_infer_type_parameters_inner(func_type.type_annotation, params);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.collect_infer_type_parameters_inner(arr.element_type, params);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.collect_infer_type_parameters_inner(elem_idx, params);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.collect_infer_type_parameters_inner(wrapped.type_node, params);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.collect_infer_type_parameters_inner(indexed.object_type, params);
                    self.collect_infer_type_parameters_inner(indexed.index_type, params);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Collect from check_type and extends_type for nested conditionals
                    self.collect_infer_type_parameters_inner(cond.check_type, params);
                    self.collect_infer_type_parameters_inner(cond.extends_type, params);
                    self.collect_infer_type_parameters_inner(cond.true_type, params);
                    self.collect_infer_type_parameters_inner(cond.false_type, params);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.collect_infer_type_parameters_inner(op.type_node, params);
                }
            }
            _ => {}
        }
    }

    /// Get type of an interface declaration.
    /// This extracts call signatures, construct signatures, and properties
    /// to build a callable type if the interface has call signatures.
    fn get_type_of_interface(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use crate::solver::{
            CallSignature as SolverCallSignature, CallableShape, IndexSignature, ObjectShape,
            PropertyInfo,
        };

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(interface) = self.ctx.arena.get_interface(node) else {
            return TypeId::ERROR; // Missing interface data - propagate error
        };

        let (_interface_type_params, interface_type_param_updates) =
            self.push_type_parameters(&interface.type_parameters);

        let mut call_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut construct_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;

        // Iterate over this interface's own members
        for &member_idx in &interface.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == CALL_SIGNATURE {
                // Extract call signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) = self.extract_params_from_signature(sig);
                    let (return_type, type_predicate) = if !sig.type_annotation.is_none() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .map(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE)
                            .unwrap_or(false);
                        if is_predicate {
                            self.return_type_and_predicate(sig.type_annotation)
                        } else {
                            (self.get_type_of_node(sig.type_annotation), None)
                        }
                    } else {
                        // Return UNKNOWN instead of ANY for missing return type annotation
                        (TypeId::UNKNOWN, None)
                    };

                    call_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == CONSTRUCT_SIGNATURE {
                // Extract construct signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) = self.extract_params_from_signature(sig);
                    let (return_type, type_predicate) = if !sig.type_annotation.is_none() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .map(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE)
                            .unwrap_or(false);
                        if is_predicate {
                            self.return_type_and_predicate(sig.type_annotation)
                        } else {
                            (self.get_type_of_node(sig.type_annotation), None)
                        }
                    } else {
                        // Return UNKNOWN instead of ANY for missing return type annotation
                        (TypeId::UNKNOWN, None)
                    };

                    construct_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == PROPERTY_SIGNATURE || member_node.kind == METHOD_SIGNATURE
            {
                // Extract property
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    if let Some(name_node) = self.ctx.arena.get(sig.name) {
                        if let Some(id_data) = self.ctx.arena.get_identifier(name_node) {
                            let type_id = if !sig.type_annotation.is_none() {
                                self.get_type_of_node(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };

                            properties.push(PropertyInfo {
                                name: self.ctx.types.intern_string(&id_data.escaped_text),
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: member_node.kind == METHOD_SIGNATURE,
                            });
                        }
                    }
                }
            } else if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if !param_data.type_annotation.is_none() {
                    self.get_type_of_node(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };
                let value_type = if !index_sig.type_annotation.is_none() {
                    self.get_type_of_node(index_sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                };
                if key_type == TypeId::NUMBER {
                    Self::merge_index_signature(&mut number_index, info);
                } else {
                    Self::merge_index_signature(&mut string_index, info);
                }
            }
        }

        let result = if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let shape = CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
            };
            self.ctx.types.callable(shape)
        } else if string_index.is_some() || number_index.is_some() {
            self.ctx.types.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
            })
        } else if !properties.is_empty() {
            self.ctx.types.object(properties)
        } else {
            TypeId::ANY
        };

        self.pop_type_parameters(interface_type_param_updates);
        self.merge_interface_heritage_types(std::slice::from_ref(&idx), result)
    }

    fn merge_interface_heritage_types(
        &mut self,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        use crate::scanner::SyntaxKind;
        use crate::solver::{TypeSubstitution, instantiate_type};

        let mut pushed_derived = false;
        let mut derived_param_updates = Vec::new();

        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };

            if !pushed_derived {
                let (_params, updates) = self.push_type_parameters(&interface.type_parameters);
                derived_param_updates = updates;
                pushed_derived = true;
            }

            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };

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

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };

                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };
                    let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                        continue;
                    };

                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    let mut base_type_params = Vec::new();
                    let mut base_param_updates = Vec::new();
                    let mut base_type = None;

                    for &base_decl_idx in &base_symbol.declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        if let Some(base_iface) = self.ctx.arena.get_interface(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_iface.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;
                            base_type = Some(self.get_type_of_symbol(base_sym_id));
                            break;
                        }
                        if let Some(base_alias) = self.ctx.arena.get_type_alias(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_alias.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;
                            base_type = Some(self.get_type_of_symbol(base_sym_id));
                            break;
                        }
                        if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_class.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;

                            // Guard against recursion when interface extends class
                            if !self.ctx.class_instance_resolution_set.insert(base_sym_id) {
                                // Recursion detected; use a type reference fallback
                                use crate::solver::{SymbolRef, TypeKey};
                                base_type = Some(
                                    self.ctx
                                        .types
                                        .intern(TypeKey::Ref(SymbolRef(base_sym_id.0))),
                                );
                            } else {
                                base_type =
                                    Some(self.get_class_instance_type(base_decl_idx, base_class));
                                self.ctx.class_instance_resolution_set.remove(&base_sym_id);
                            }
                            break;
                        }
                    }

                    if base_type.is_none() && !base_symbol.value_declaration.is_none() {
                        let base_decl_idx = base_symbol.value_declaration;
                        if let Some(base_node) = self.ctx.arena.get(base_decl_idx) {
                            if let Some(base_iface) = self.ctx.arena.get_interface(base_node) {
                                let (params, updates) =
                                    self.push_type_parameters(&base_iface.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;
                                base_type = Some(self.get_type_of_symbol(base_sym_id));
                            } else if let Some(base_alias) =
                                self.ctx.arena.get_type_alias(base_node)
                            {
                                let (params, updates) =
                                    self.push_type_parameters(&base_alias.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;
                                base_type = Some(self.get_type_of_symbol(base_sym_id));
                            } else if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                                let (params, updates) =
                                    self.push_type_parameters(&base_class.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;

                                // Guard against recursion when interface extends class
                                if !self.ctx.class_instance_resolution_set.insert(base_sym_id) {
                                    // Recursion detected; use a type reference fallback
                                    use crate::solver::{SymbolRef, TypeKey};
                                    base_type = Some(
                                        self.ctx
                                            .types
                                            .intern(TypeKey::Ref(SymbolRef(base_sym_id.0))),
                                    );
                                } else {
                                    base_type = Some(
                                        self.get_class_instance_type(base_decl_idx, base_class),
                                    );
                                    self.ctx.class_instance_resolution_set.remove(&base_sym_id);
                                }
                            }
                        }
                    }

                    let Some(mut base_type) = base_type else {
                        continue;
                    };

                    if type_args.len() < base_type_params.len() {
                        for param in base_type_params.iter().skip(type_args.len()) {
                            let fallback =
                                param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                            type_args.push(fallback);
                        }
                    }
                    if type_args.len() > base_type_params.len() {
                        type_args.truncate(base_type_params.len());
                    }

                    let substitution = TypeSubstitution::from_args(&base_type_params, &type_args);
                    base_type = instantiate_type(self.ctx.types, base_type, &substitution);

                    self.pop_type_parameters(base_param_updates);

                    derived_type = self.merge_interface_types(derived_type, base_type);
                }
            }
        }

        if pushed_derived {
            self.pop_type_parameters(derived_param_updates);
        }

        derived_type
    }

    fn merge_interface_types(&mut self, derived: TypeId, base: TypeId) -> TypeId {
        use crate::solver::{CallableShape, ObjectShape, TypeKey};

        if derived == base {
            return derived;
        }

        let derived_key = self.ctx.types.lookup(derived);
        let base_key = self.ctx.types.lookup(base);

        match (derived_key, base_key) {
            (Some(TypeKey::Callable(derived_shape_id)), Some(TypeKey::Callable(base_shape_id))) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let mut call_signatures = derived_shape.call_signatures.clone();
                call_signatures.extend(base_shape.call_signatures.iter().cloned());
                let mut construct_signatures = derived_shape.construct_signatures.clone();
                construct_signatures.extend(base_shape.construct_signatures.iter().cloned());
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (Some(TypeKey::Callable(derived_shape_id)), Some(TypeKey::Object(base_shape_id))) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                })
            }
            (
                Some(TypeKey::Callable(derived_shape_id)),
                Some(TypeKey::ObjectWithIndex(base_shape_id)),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (Some(TypeKey::Object(derived_shape_id)), Some(TypeKey::Callable(base_shape_id))) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            (
                Some(TypeKey::ObjectWithIndex(derived_shape_id)),
                Some(TypeKey::Callable(base_shape_id)),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (Some(TypeKey::Object(derived_shape_id)), Some(TypeKey::Object(base_shape_id))) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object(properties)
            }
            (
                Some(TypeKey::Object(derived_shape_id)),
                Some(TypeKey::ObjectWithIndex(base_shape_id)),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            (
                Some(TypeKey::ObjectWithIndex(derived_shape_id)),
                Some(TypeKey::Object(base_shape_id)),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                })
            }
            (
                Some(TypeKey::ObjectWithIndex(derived_shape_id)),
                Some(TypeKey::ObjectWithIndex(base_shape_id)),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                })
            }
            (_, Some(TypeKey::Intersection(_))) | (Some(TypeKey::Intersection(_)), _) => {
                self.ctx.types.intersection2(derived, base)
            }
            _ => derived,
        }
    }

    fn merge_properties(
        &self,
        derived: &[crate::solver::PropertyInfo],
        base: &[crate::solver::PropertyInfo],
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::interner::Atom;
        use rustc_hash::FxHashMap;

        let mut merged: FxHashMap<Atom, crate::solver::PropertyInfo> = FxHashMap::default();
        for prop in base {
            merged.insert(prop.name, prop.clone());
        }
        for prop in derived {
            merged.insert(prop.name, prop.clone());
        }
        merged.into_values().collect()
    }

    /// Helper to extract parameters from a SignatureData.
    fn extract_params_from_signature(
        &mut self,
        sig: &crate::parser::thin_node::SignatureData,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        use crate::solver::ParamInfo;
        use std::sync::Arc;

        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        let mut params = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let name: Option<Atom> = if let Some(name_node) = self.ctx.arena.get(param.name) {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    Some(self.ctx.types.intern_string(&name_data.escaped_text))
                } else {
                    None
                }
            } else {
                None
            };

            let type_id = if !param.type_annotation.is_none() {
                self.get_type_of_node(param.type_annotation)
            } else {
                TypeId::ANY
            };

            let optional = param.question_token || !param.initializer.is_none();
            let rest = param.dot_dot_dot_token;

            if let Some(name_atom) = name {
                if name_atom == this_atom {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    continue;
                }
            }

            params.push(ParamInfo {
                name,
                type_id,
                optional,
                rest,
            });
        }

        (params, this_type)
    }

    /// Helper to extract parameters from a parameter list.
    fn extract_params_from_parameter_list(
        &mut self,
        params_list: &crate::parser::NodeList,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        use crate::solver::ParamInfo;

        let mut params = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let type_id = if !param.type_annotation.is_none() {
                self.get_type_from_type_node(param.type_annotation)
            } else {
                TypeId::ANY
            };

            let name_node = self.ctx.arena.get(param.name);
            if let Some(name_node) = name_node {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    continue;
                }
            }

            let name: Option<Atom> = if let Some(name_node) = name_node {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    Some(self.ctx.types.intern_string(&name_data.escaped_text))
                } else {
                    None
                }
            } else {
                None
            };

            let optional = param.question_token || !param.initializer.is_none();
            let rest = param.dot_dot_dot_token;

            if let Some(name_atom) = name {
                if name_atom == this_atom {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    continue;
                }
            }

            params.push(ParamInfo {
                name,
                type_id,
                optional,
                rest,
            });
        }

        (params, this_type)
    }

    fn type_predicate_target(
        &self,
        param_name: NodeIndex,
    ) -> Option<crate::solver::TypePredicateTarget> {
        use crate::solver::TypePredicateTarget;

        let node = self.ctx.arena.get(param_name)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.ctx.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.ctx.types.intern_string(&ident.escaped_text))
        })
    }

    fn return_type_and_predicate(
        &mut self,
        type_annotation: NodeIndex,
    ) -> (TypeId, Option<crate::solver::TypePredicate>) {
        use crate::solver::TypePredicate;

        if type_annotation.is_none() {
            // Return UNKNOWN instead of ANY to enforce strict type checking
            return (TypeId::UNKNOWN, None);
        }

        let Some(node) = self.ctx.arena.get(type_annotation) else {
            return (TypeId::UNKNOWN, None);
        };

        if node.kind != syntax_kind_ext::TYPE_PREDICATE {
            return (self.get_type_from_type_node(type_annotation), None);
        }

        let Some(data) = self.ctx.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node.is_none() {
            None
        } else {
            Some(self.get_type_from_type_node(data.type_node))
        };

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
        };

        (return_type, Some(predicate))
    }

    fn return_type_and_predicate_in_type_literal(
        &mut self,
        type_annotation: NodeIndex,
    ) -> (TypeId, Option<crate::solver::TypePredicate>) {
        use crate::solver::TypePredicate;

        if type_annotation.is_none() {
            // Return UNKNOWN instead of ANY for missing type annotation
            return (TypeId::UNKNOWN, None);
        }

        let Some(node) = self.ctx.arena.get(type_annotation) else {
            // Return UNKNOWN instead of ANY for missing node
            return (TypeId::UNKNOWN, None);
        };

        if node.kind != syntax_kind_ext::TYPE_PREDICATE {
            return (
                self.get_type_from_type_node_in_type_literal(type_annotation),
                None,
            );
        }

        let Some(data) = self.ctx.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node.is_none() {
            None
        } else {
            Some(self.get_type_from_type_node_in_type_literal(data.type_node))
        };

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
        };

        (return_type, Some(predicate))
    }

    fn call_signature_from_function(
        &mut self,
        func: &crate::parser::thin_node::FunctionData,
    ) -> crate::solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&func.parameters);
        let (return_type, type_predicate) = self.return_type_and_predicate(func.type_annotation);

        self.pop_type_parameters(type_param_updates);

        crate::solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
        }
    }

    fn call_signature_from_method(
        &mut self,
        method: &crate::parser::thin_node::MethodDeclData,
    ) -> crate::solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let (return_type, type_predicate) = self.return_type_and_predicate(method.type_annotation);

        self.pop_type_parameters(type_param_updates);

        crate::solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
        }
    }

    fn call_signature_from_constructor(
        &mut self,
        ctor: &crate::parser::thin_node::ConstructorData,
        instance_type: TypeId,
        class_type_params: &[crate::solver::TypeParamInfo],
    ) -> crate::solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&ctor.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&ctor.parameters);

        self.pop_type_parameters(type_param_updates);

        let mut all_type_params = Vec::with_capacity(class_type_params.len() + type_params.len());
        all_type_params.extend_from_slice(class_type_params);
        all_type_params.extend(type_params);

        crate::solver::CallSignature {
            type_params: all_type_params,
            params,
            this_type,
            return_type: instance_type,
            type_predicate: None,
        }
    }

    fn get_class_instance_type(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::thin_node::ClassData,
    ) -> TypeId {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.get_class_instance_type_inner(class_idx, class, &mut visited)
    }

    fn get_class_instance_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::thin_node::ClassData,
        visited: &mut rustc_hash::FxHashSet<crate::binder::SymbolId>,
    ) -> TypeId {
        use crate::scanner::SyntaxKind;
        use crate::solver::{
            CallSignature, CallableShape, IndexSignature, ObjectShape, PropertyInfo, TypeKey,
            TypeLowering, TypeSubstitution, instantiate_type,
        };
        use rustc_hash::FxHashMap;

        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        if let Some(sym_id) = current_sym {
            if !visited.insert(sym_id) {
                return TypeId::ERROR; // Circular reference - propagate error
            }
        }

        struct MethodAggregate {
            overload_signatures: Vec<CallSignature>,
            impl_signatures: Vec<CallSignature>,
            overload_optional: bool,
            impl_optional: bool,
        }

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
        }

        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut methods: FxHashMap<Atom, MethodAggregate> = FxHashMap::default();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut has_nominal_members = false;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&prop.modifiers, prop.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let type_id = if !prop.type_annotation.is_none() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if !prop.initializer.is_none() {
                        self.get_type_of_node(prop.initializer)
                    } else {
                        // Return UNKNOWN instead of ANY for property without annotation or initializer
                        TypeId::UNKNOWN
                    };

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: self.has_readonly_modifier(&prop.modifiers),
                            is_method: false,
                        },
                    );
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&method.modifiers, method.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let signature = self.call_signature_from_method(method);
                    let entry = methods.entry(name_atom).or_insert(MethodAggregate {
                        overload_signatures: Vec::new(),
                        impl_signatures: Vec::new(),
                        overload_optional: false,
                        impl_optional: false,
                    });
                    if method.body.is_none() {
                        entry.overload_signatures.push(signature);
                        entry.overload_optional |= method.question_token;
                    } else {
                        entry.impl_signatures.push(signature);
                        entry.impl_optional |= method.question_token;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&accessor.modifiers, accessor.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                        getter: None,
                        setter: None,
                    });

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if !accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        };
                        entry.getter = Some(getter_type);
                    } else {
                        let setter_type = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| {
                                if !param.type_annotation.is_none() {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        entry.setter = Some(setter_type);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        if self.has_private_modifier(&param.modifiers)
                            || self.has_protected_modifier(&param.modifiers)
                        {
                            has_nominal_members = true;
                        }
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        if properties.contains_key(&name_atom) {
                            continue;
                        }
                        let type_id = if !param.type_annotation.is_none() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else if !param.initializer.is_none() {
                            self.get_type_of_node(param.initializer)
                        } else {
                            TypeId::ANY
                        };
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: param.question_token,
                                readonly: self.has_readonly_modifier(&param.modifiers),
                                is_method: false,
                            },
                        );
                    }
                }
                k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&index_sig.modifiers) {
                        continue;
                    }

                    let param_idx = index_sig
                        .parameters
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };

                    let key_type = if param.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(param.type_annotation)
                    };
                    let value_type = if index_sig.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    };
                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);

                    let index = IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                    };

                    if key_type == TypeId::NUMBER {
                        Self::merge_index_signature(&mut number_index, index);
                    } else {
                        Self::merge_index_signature(&mut string_index, index);
                    }
                }
                _ => {}
            }
        }

        for (name, accessor) in accessors {
            if methods.contains_key(&name) {
                continue;
            }
            let read_type = accessor.getter.or(accessor.setter).unwrap_or(TypeId::UNKNOWN);
            let write_type = accessor.setter.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                },
            );
        }

        for (name, method) in methods {
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (method.overload_signatures, method.overload_optional)
            } else {
                (method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }
            let type_id = self.ctx.types.callable(CallableShape {
                call_signatures: signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
            });
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional,
                    readonly: false,
                    is_method: true,
                },
            );
        }

        if has_nominal_members {
            let brand_name = if let Some(sym_id) = current_sym {
                format!("__private_brand_{}", sym_id.0)
            } else {
                format!("__private_brand_node_{}", class_idx.0)
            };
            let brand_atom = self.ctx.types.intern_string(&brand_name);
            properties.entry(brand_atom).or_insert(PropertyInfo {
                name: brand_atom,
                // Use UNKNOWN instead of ANY for brand property type
                type_id: TypeId::UNKNOWN,
                write_type: TypeId::UNKNOWN,
                optional: false,
                readonly: true,
                is_method: false,
            });
        }

        // Merge base class instance properties (derived members take precedence).
        if let Some(ref heritage_clauses) = class.heritage_clauses {
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
                    break;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    break;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let base_sym_id = match self.resolve_heritage_symbol(expr_idx) {
                    Some(base_sym_id) => base_sym_id,
                    None => {
                        if let Some(base_instance_type) =
                            self.base_instance_type_from_expression(expr_idx, type_arguments)
                        {
                            self.merge_base_instance_properties(
                                base_instance_type,
                                &mut properties,
                                &mut string_index,
                                &mut number_index,
                            );
                        }
                        break;
                    }
                };
                if visited.contains(&base_sym_id) {
                    break;
                }
                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    break;
                };

                let mut base_class_idx = None;
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_class(node).is_some() {
                            base_class_idx = Some(decl_idx);
                            break;
                        }
                    }
                }
                if base_class_idx.is_none() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_class(node).is_some() {
                            base_class_idx = Some(decl_idx);
                        }
                    }
                }
                let Some(base_class_idx) = base_class_idx else {
                    if let Some(base_instance_type) =
                        self.base_instance_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_base_instance_properties(
                            base_instance_type,
                            &mut properties,
                            &mut string_index,
                            &mut number_index,
                        );
                    }
                    break;
                };
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_class.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let base_instance_type =
                    self.get_class_instance_type_inner(base_class_idx, base_class, visited);
                let substitution = TypeSubstitution::from_args(&base_type_params, &type_args);
                let base_instance_type =
                    instantiate_type(self.ctx.types, base_instance_type, &substitution);
                self.pop_type_parameters(base_type_param_updates);

                if let Some(
                    TypeKey::Object(base_shape_id) | TypeKey::ObjectWithIndex(base_shape_id),
                ) = self.ctx.types.lookup(base_instance_type)
                {
                    let base_shape = self.ctx.types.object_shape(base_shape_id);
                    for base_prop in base_shape.properties.iter() {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    if let Some(ref idx) = base_shape.string_index {
                        Self::merge_index_signature(&mut string_index, idx.clone());
                    }
                    if let Some(ref idx) = base_shape.number_index {
                        Self::merge_index_signature(&mut number_index, idx.clone());
                    }
                }

                break;
            }
        }

        // Merge implemented interface properties (class members take precedence).
        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                    let Some(interface_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };

                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    let mut interface_type = self.type_reference_symbol_type(interface_sym_id);
                    let interface_type_params = self.get_type_params_for_symbol(interface_sym_id);

                    if type_args.len() < interface_type_params.len() {
                        for param in interface_type_params.iter().skip(type_args.len()) {
                            let fallback =
                                param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                            type_args.push(fallback);
                        }
                    }
                    if type_args.len() > interface_type_params.len() {
                        type_args.truncate(interface_type_params.len());
                    }

                    if !interface_type_params.is_empty() {
                        let substitution =
                            TypeSubstitution::from_args(&interface_type_params, &type_args);
                        interface_type =
                            instantiate_type(self.ctx.types, interface_type, &substitution);
                    }

                    match self.ctx.types.lookup(interface_type) {
                        Some(TypeKey::Object(shape_id))
                        | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                            let shape = self.ctx.types.object_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                            if let Some(ref idx) = shape.string_index {
                                Self::merge_index_signature(&mut string_index, idx.clone());
                            }
                            if let Some(ref idx) = shape.number_index {
                                Self::merge_index_signature(&mut number_index, idx.clone());
                            }
                        }
                        Some(TypeKey::Callable(shape_id)) => {
                            let shape = self.ctx.types.callable_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Merge interface declarations for class/interface merging (class members take precedence).
        if let Some(sym_id) = current_sym {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let interface_decls: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .copied()
                    .filter(|&decl_idx| {
                        self.ctx
                            .arena
                            .get(decl_idx)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .is_some()
                    })
                    .collect();

                if !interface_decls.is_empty() {
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type = lowering.lower_interface_declarations(&interface_decls);
                    let interface_type =
                        self.merge_interface_heritage_types(&interface_decls, interface_type);

                    match self.ctx.types.lookup(interface_type) {
                        Some(TypeKey::Object(shape_id))
                        | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                            let shape = self.ctx.types.object_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                            if let Some(ref idx) = shape.string_index {
                                Self::merge_index_signature(&mut string_index, idx.clone());
                            }
                            if let Some(ref idx) = shape.number_index {
                                Self::merge_index_signature(&mut number_index, idx.clone());
                            }
                        }
                        Some(TypeKey::Callable(shape_id)) => {
                            let shape = self.ctx.types.callable_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Classes inherit Object members (toString, hasOwnProperty, etc.).
        if let Some(object_type) = self.resolve_lib_type_by_name("Object") {
            match self.ctx.types.lookup(object_type) {
                Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                    let shape = self.ctx.types.object_shape(shape_id);
                    for prop in shape.properties.iter() {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                    if let Some(ref idx) = shape.string_index {
                        Self::merge_index_signature(&mut string_index, idx.clone());
                    }
                    if let Some(ref idx) = shape.number_index {
                        Self::merge_index_signature(&mut number_index, idx.clone());
                    }
                }
                Some(TypeKey::Callable(shape_id)) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    for prop in shape.properties.iter() {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                }
                _ => {}
            }
        }

        let props: Vec<PropertyInfo> = properties.into_values().collect();
        let mut instance_type = if string_index.is_some() || number_index.is_some() {
            self.ctx.types.object_with_index(ObjectShape {
                properties: props,
                string_index,
                number_index,
            })
        } else {
            self.ctx.types.object(props)
        };

        if let Some(sym_id) = current_sym {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let interface_decls: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .copied()
                    .filter(|decl_idx| {
                        self.ctx
                            .arena
                            .get(*decl_idx)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .is_some()
                    })
                    .collect();

                if !interface_decls.is_empty() {
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type = lowering.lower_interface_declarations(&interface_decls);
                    let interface_type =
                        self.merge_interface_heritage_types(&interface_decls, interface_type);
                    instance_type = self.merge_interface_types(instance_type, interface_type);
                }
            }
            visited.remove(&sym_id);
        }
        instance_type
    }

    fn get_class_constructor_type(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::thin_node::ClassData,
    ) -> TypeId {
        use crate::scanner::SyntaxKind;
        use crate::solver::{
            CallSignature, CallableShape, PropertyInfo, TypeKey, TypeSubstitution, instantiate_type,
        };
        use rustc_hash::FxHashMap;

        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);
        let (class_type_params, type_param_updates) =
            self.push_type_parameters(&class.type_parameters);
        let instance_type = self.get_class_instance_type(class_idx, class);

        struct MethodAggregate {
            overload_signatures: Vec<CallSignature>,
            impl_signatures: Vec<CallSignature>,
            overload_optional: bool,
            impl_optional: bool,
        }

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
        }

        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut methods: FxHashMap<Atom, MethodAggregate> = FxHashMap::default();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut static_string_index: Option<crate::solver::IndexSignature> = None;
        let mut static_number_index: Option<crate::solver::IndexSignature> = None;
        let mut has_static_nominal_members = false;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&prop.modifiers, prop.name) {
                        has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let type_id = if !prop.type_annotation.is_none() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if !prop.initializer.is_none() {
                        self.get_type_of_node(prop.initializer)
                    } else {
                        // Return UNKNOWN instead of ANY for property without annotation or initializer
                        TypeId::UNKNOWN
                    };

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: self.has_readonly_modifier(&prop.modifiers),
                            is_method: false,
                        },
                    );
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&method.modifiers, method.name) {
                        has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let signature = self.call_signature_from_method(method);
                    let entry = methods.entry(name_atom).or_insert(MethodAggregate {
                        overload_signatures: Vec::new(),
                        impl_signatures: Vec::new(),
                        overload_optional: false,
                        impl_optional: false,
                    });
                    if method.body.is_none() {
                        entry.overload_signatures.push(signature);
                        entry.overload_optional |= method.question_token;
                    } else {
                        entry.impl_signatures.push(signature);
                        entry.impl_optional |= method.question_token;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&accessor.modifiers, accessor.name) {
                        has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                        getter: None,
                        setter: None,
                    });

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if !accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        };
                        entry.getter = Some(getter_type);
                    } else {
                        let setter_type = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| {
                                if !param.type_annotation.is_none() {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        entry.setter = Some(setter_type);
                    }
                }
                k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&index_sig.modifiers) {
                        continue;
                    }
                    // Determine key type from the parameter
                    let key_type = index_sig
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                        .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                        .and_then(|param| {
                            if !param.type_annotation.is_none() {
                                Some(self.get_type_from_type_node(param.type_annotation))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(TypeId::STRING);

                    let value_type = if !index_sig.type_annotation.is_none() {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    } else {
                        TypeId::ANY
                    };

                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);

                    let idx_sig = crate::solver::IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                    };

                    // Check if key is string or number type
                    if key_type == TypeId::NUMBER {
                        static_number_index = Some(idx_sig);
                    } else {
                        // Default to string index for string or symbol keys
                        static_string_index = Some(idx_sig);
                    }
                }
                _ => {}
            }
        }

        for (name, accessor) in accessors {
            if methods.contains_key(&name) {
                continue;
            }
            let read_type = accessor.getter.or(accessor.setter).unwrap_or(TypeId::UNKNOWN);
            let write_type = accessor.setter.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                },
            );
        }

        for (name, method) in methods {
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (method.overload_signatures, method.overload_optional)
            } else {
                (method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }
            let type_id = self.ctx.types.callable(CallableShape {
                call_signatures: signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
            });
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional,
                    readonly: false,
                    is_method: true,
                },
            );
        }

        // Merge base class static properties (derived members take precedence).
        if let Some(ref heritage_clauses) = class.heritage_clauses {
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
                    break;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    break;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let base_sym_id = match self.resolve_heritage_symbol(expr_idx) {
                    Some(base_sym_id) => base_sym_id,
                    None => {
                        if let Some(base_constructor_type) =
                            self.base_constructor_type_from_expression(expr_idx, type_arguments)
                        {
                            self.merge_constructor_properties_from_type(
                                base_constructor_type,
                                &mut properties,
                            );
                        }
                        break;
                    }
                };
                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    break;
                };

                let mut base_class_idx = None;
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_class(node).is_some() {
                            base_class_idx = Some(decl_idx);
                            break;
                        }
                    }
                }
                if base_class_idx.is_none() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_class(node).is_some() {
                            base_class_idx = Some(decl_idx);
                        }
                    }
                }
                let Some(base_class_idx) = base_class_idx else {
                    if let Some(base_constructor_type) =
                        self.base_constructor_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_constructor_properties_from_type(
                            base_constructor_type,
                            &mut properties,
                        );
                    }
                    break;
                };
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_class.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let base_constructor_type =
                    self.get_class_constructor_type(base_class_idx, base_class);
                let substitution = TypeSubstitution::from_args(&base_type_params, &type_args);
                let base_constructor_type =
                    instantiate_type(self.ctx.types, base_constructor_type, &substitution);
                self.pop_type_parameters(base_type_param_updates);

                if let Some(TypeKey::Callable(base_shape_id)) =
                    self.ctx.types.lookup(base_constructor_type)
                {
                    let base_shape = self.ctx.types.callable_shape(base_shape_id);
                    for base_prop in base_shape.properties.iter() {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                }

                break;
            }
        }

        let mut has_overloads = false;
        let mut constructor_access: Option<MemberAccessLevel> = None;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(member_node) {
                    if self.has_private_modifier(&ctor.modifiers) {
                        constructor_access = Some(MemberAccessLevel::Private);
                    } else if self.has_protected_modifier(&ctor.modifiers)
                        && constructor_access != Some(MemberAccessLevel::Private)
                    {
                        constructor_access = Some(MemberAccessLevel::Protected);
                    }
                    if ctor.body.is_none() {
                        has_overloads = true;
                    }
                }
            }
        }

        let mut construct_signatures = Vec::new();
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };

            if has_overloads {
                if ctor.body.is_none() {
                    construct_signatures.push(self.call_signature_from_constructor(
                        ctor,
                        instance_type,
                        &class_type_params,
                    ));
                }
            } else {
                construct_signatures.push(self.call_signature_from_constructor(
                    ctor,
                    instance_type,
                    &class_type_params,
                ));
                break;
            }
        }

        if construct_signatures.is_empty() {
            construct_signatures.push(CallSignature {
                type_params: class_type_params,
                params: Vec::new(),
                this_type: None,
                return_type: instance_type,
                type_predicate: None,
            });
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        self.pop_type_parameters(type_param_updates);

        let constructor_type = self.ctx.types.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures,
            properties,
            string_index: static_string_index,
            number_index: static_number_index,
        });

        if let Some(level) = constructor_access {
            match level {
                MemberAccessLevel::Private => {
                    self.ctx.private_constructor_types.insert(constructor_type);
                }
                MemberAccessLevel::Protected => {
                    self.ctx
                        .protected_constructor_types
                        .insert(constructor_type);
                }
            }
        }

        if is_abstract_class {
            self.ctx.abstract_constructor_types.insert(constructor_type);
        }

        constructor_type
    }

    // =========================================================================
    // Type Resolution - Specific Node Types
    // =========================================================================

    /// Get type of identifier, with control flow analysis for narrowing.
    fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            let flags = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map(|symbol| symbol.flags)
                .unwrap_or(0);
            let has_type = (flags & symbol_flags::TYPE) != 0;
            let has_value = (flags & symbol_flags::VALUE) != 0;
            if has_type && !has_value {
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            // 1. Static block TDZ - variable used in static block before its declaration
            // 2. Computed property TDZ - variable used in computed property name before its declaration
            // 3. Heritage clause TDZ - variable used in extends/implements before its declaration
            if self.is_variable_used_before_declaration_in_static_block(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
            } else if self.is_variable_used_before_declaration_in_computed_property(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
            } else if self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx) {
                self.error_variable_used_before_assigned_at(name, idx);
            } else if self.should_check_definite_assignment(sym_id, idx)
                && !self.is_definitely_assigned_at(idx)
            {
                self.error_variable_used_before_assigned_at(name, idx);
            }
            return self.apply_flow_narrowing(idx, declared_type);
        }

        // Intrinsic names - use constant TypeIds
        match name.as_str() {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            // Symbol constructor - synthesize proper type for call signature validation
            "Symbol" => self.get_symbol_constructor_type(),
            _ if self.is_known_global_value_name(name) => {
                // Return ANY for known globals to allow property access
                TypeId::ANY
            }
            _ => {
                // Check if we're inside a class and the name matches a static member (error 2662)
                // Clone values to avoid borrow issues
                if let Some(ref class_info) = self.ctx.enclosing_class.clone() {
                    if self.is_static_member(&class_info.member_nodes, name) {
                        self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
                        return TypeId::ERROR;
                    }
                }
                // Report "cannot find name" error
                self.error_cannot_find_name_at(name, idx);
                TypeId::ERROR
            }
        }
    }

    /// Synthesize the Symbol constructor type.
    ///
    /// Returns a callable type with signature: `Symbol(description?: string | number): symbol`
    /// Note: Symbol cannot be constructed with `new`, so no construct signatures.
    fn get_symbol_constructor_type(&self) -> TypeId {
        use crate::solver::{CallSignature, CallableShape, ParamInfo, PropertyInfo};

        // Parameter: description?: string | number
        let description_param_type = self.ctx.types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let description_param = ParamInfo {
            name: Some(self.ctx.types.intern_string("description")),
            type_id: description_param_type,
            optional: true,
            rest: false,
        };

        let call_signature = CallSignature {
            type_params: vec![],
            params: vec![description_param],
            this_type: None,
            return_type: TypeId::SYMBOL,
            type_predicate: None,
        };

        let well_known = [
            "iterator",
            "asyncIterator",
            "hasInstance",
            "isConcatSpreadable",
            "match",
            "matchAll",
            "replace",
            "search",
            "split",
            "species",
            "toPrimitive",
            "toStringTag",
            "unscopables",
            "dispose",
            "asyncDispose",
            "metadata",
        ];

        let mut properties = Vec::new();
        for name in well_known {
            let name_atom = self.ctx.types.intern_string(name);
            properties.push(PropertyInfo {
                name: name_atom,
                type_id: TypeId::SYMBOL,
                write_type: TypeId::SYMBOL,
                optional: false,
                readonly: true,
                is_method: false,
            });
        }

        self.ctx.types.callable(CallableShape {
            call_signatures: vec![call_signature],
            construct_signatures: Vec::new(),
            properties,
            string_index: None,
            number_index: None,
        })
    }

    /// Apply control flow narrowing to a type at a specific identifier usage.
    ///
    /// This walks backwards through the control flow graph to determine what
    /// type guards (typeof, null checks, etc.) have been applied.
    fn apply_flow_narrowing(&self, idx: NodeIndex, declared_type: TypeId) -> TypeId {
        // Get the flow node for this identifier usage
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return declared_type, // No flow info - use declared type
        };

        // Skip narrowing for non-union types (nothing to narrow)
        // Also skip for primitives that can't be narrowed further
        if !self.is_narrowable_type(declared_type) {
            return declared_type;
        }

        // Create a flow analyzer and apply narrowing
        let analyzer = FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        );

        analyzer.get_flow_type(idx, declared_type, flow_node)
    }

    /// Check flow-based usage of an identifier.
    ///
    /// This method combines:
    /// - Definite assignment checking (TS2454 errors)
    /// - Type narrowing based on control flow
    ///
    /// # Arguments
    /// * `idx` - The AST node index of the identifier reference
    /// * `declared_type` - The declared type of the identifier
    /// * `sym_id` - The symbol ID of the identifier
    ///
    /// # Returns
    /// The narrowed type if the identifier is definitely assigned, otherwise
    /// the declared type (errors are reported separately).
    ///
    /// # Errors
    /// Emits TS2454 error if the variable is used before being definitely assigned.
    pub fn check_flow_usage(
        &mut self,
        idx: NodeIndex,
        declared_type: TypeId,
        sym_id: SymbolId,
    ) -> TypeId {
        // Check definite assignment for block-scoped variables without initializers
        if self.should_check_definite_assignment(sym_id, idx) {
            if !self.is_definitely_assigned_at(idx) {
                // Report TS2454 error: Variable used before assignment
                self.emit_definite_assignment_error(idx, sym_id);
                // Return declared type to avoid cascading errors
                return declared_type;
            }
        }

        // Apply type narrowing based on control flow
        self.apply_flow_narrowing(idx, declared_type)
    }

    /// Emit TS2454 error for variable used before definite assignment.
    ///
    /// # Arguments
    /// * `idx` - The AST node where the variable is used
    /// * `sym_id` - The symbol ID of the variable
    fn emit_definite_assignment_error(&mut self, idx: NodeIndex, sym_id: SymbolId) {
        // Get the variable name for the error message
        let name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());

        // Get the location for error reporting
        let Some(node) = self.ctx.arena.get(idx) else {
            // If the node doesn't exist in the arena, emit error with position 0
            self.ctx.diagnostics.push(Diagnostic::error(
                "file".to_string(), // TODO: Get actual file name
                0,
                0,
                format!("Variable '{}' is used before being assigned", name),
                2454, // TS2454
            ));
            return;
        };
        let start = node.pos;
        let length = node.end - node.pos;

        self.ctx.diagnostics.push(Diagnostic::error(
            "file".to_string(), // TODO: Get actual file name
            start,
            length,
            format!("Variable '{}' is used before being assigned", name),
            2454, // TS2454
        ));
    }

    /// Check if a type can be narrowed (unions, nullable types, etc.)
    fn is_narrowable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        // Check if it's a union type or a type parameter (which can be narrowed)
        if let Some(key) = self.ctx.types.lookup(type_id) {
            if matches!(
                key,
                TypeKey::Union(_) | TypeKey::TypeParameter(_) | TypeKey::Infer(_)
            ) {
                return true;
            }
        }

        // Could also check for types that include null/undefined
        // For now, only narrow unions
        false
    }

    /// Check if a type is callable (has call signatures).
    /// Callable types allow arbitrary property access because functions are objects at runtime.
    fn is_callable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        if let Some(key) = self.ctx.types.lookup(type_id) {
            match key {
                TypeKey::Callable(shape_id) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    // A type is callable if it has at least one call signature
                    !shape.call_signatures.is_empty()
                }
                TypeKey::Function(_) => {
                    // Function types are always callable
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn should_check_definite_assignment(&mut self, sym_id: SymbolId, idx: NodeIndex) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }
        // Check both block-scoped (let/const) and function-scoped (var) variables
        // definite assignment should apply to all typed variables without initializers
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0
            && (symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) == 0
        {
            return false;
        }

        if self.symbol_is_parameter(sym_id) {
            return false;
        }

        if self.symbol_has_definite_assignment_assertion(sym_id) {
            return false;
        }

        if self.is_for_in_of_assignment_target(idx) {
            return false;
        }

        // Skip if the variable declaration has an initializer
        if self.symbol_has_initializer(sym_id) {
            return false;
        }

        // Skip if the variable is in an ambient context (declare var x: T)
        if self.symbol_is_in_ambient_context(sym_id) {
            return false;
        }

        // Skip if the variable is captured in a closure (used in a different function).
        // TypeScript doesn't check definite assignment for variables captured in non-IIFE closures
        // because the closure might be called later when the variable is assigned.
        if self.is_variable_captured_in_closure(sym_id, idx) {
            return false;
        }

        // Skip definite assignment check for variables whose types allow uninitialized use:
        // - Literal types: `let key: "a"` - the type restricts to a single literal
        // - Union of literals: `let key: "a" | "b"` - all possible values are literals
        // - Types with undefined: `let obj: Foo | undefined` - undefined is the default
        if self.symbol_type_allows_uninitialized(sym_id) {
            return false;
        }

        true
    }

    /// Check if a variable symbol is in an ambient context (declared with `declare`).
    fn symbol_is_in_ambient_context(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Quick check: if lib_contexts is not empty and symbol is not in main binder's arena,
        // it's likely from lib.d.ts which is all ambient
        if !self.ctx.lib_contexts.is_empty() {
            // Check if symbol exists in main binder's symbol arena
            let is_from_lib = self.ctx.binder.get_symbols().get(sym_id).is_none();
            if is_from_lib {
                // Symbol is from lib.d.ts, which is all ambient (declare statements)
                return true;
            }
        }

        for &decl_idx in &symbol.declarations {
            // Check if the variable statement has a declare modifier
            if let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) {
                if let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) {
                    if let Some(var_stmt) = self.ctx.arena.get_variable(var_stmt_node) {
                        if self.has_declare_modifier(&var_stmt.modifiers) {
                            return true;
                        }
                    }
                }
            }

            // Also check node flags for AMBIENT
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if (node.flags as u32) & crate::parser::node_flags::AMBIENT != 0 {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a variable is captured in a closure (used in a different function than its declaration).
    /// TypeScript does not check definite assignment for variables captured in non-IIFE closures.
    fn is_variable_captured_in_closure(&self, sym_id: SymbolId, usage_idx: NodeIndex) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Get the enclosing function for the usage
        let usage_function = self.find_enclosing_function(usage_idx);

        // Get the enclosing function for the variable's declaration
        for &decl_idx in &symbol.declarations {
            let decl_function = self.find_enclosing_function(decl_idx);

            // If the usage is in a different function than the declaration,
            // the variable is captured in a closure
            if usage_function != decl_function {
                return true;
            }
        }

        false
    }

    /// Find the enclosing function-like node for a given node.
    /// Returns Some(NodeIndex) if inside a function, None if at module/global scope.
    fn find_enclosing_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.is_function_like() {
                    return Some(current);
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the enclosing NON-ARROW function for a given node.
    /// Returns Some(NodeIndex) if inside a non-arrow function (function declaration/expression),
    /// None if at module/global scope or only inside arrow functions.
    ///
    /// This is used for `this` type checking: arrow functions capture `this` from their
    /// enclosing scope, so we need to skip past them to find the actual function that
    /// defines the `this` context.
    fn find_enclosing_non_arrow_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use crate::parser::syntax_kind_ext::*;
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check for non-arrow functions that define their own `this` context
                if node.kind == FUNCTION_DECLARATION
                    || node.kind == FUNCTION_EXPRESSION
                    || node.kind == METHOD_DECLARATION
                    || node.kind == CONSTRUCTOR
                    || node.kind == GET_ACCESSOR
                    || node.kind == SET_ACCESSOR
                {
                    return Some(current);
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if a variable symbol can be used without initialization.
    /// This includes:
    /// 1. Literal types (e.g., `let key: "a"`)
    /// 2. Unions of literals (e.g., `let key: "a" | "b"`)
    /// 3. Types that include `undefined` (e.g., `let obj: Foo | undefined`)
    /// 4. `any` type - TypeScript doesn't check definite assignment for `any`
    /// 5. `typeof undefined` - resolves to `undefined`, which allows uninitialized use
    fn symbol_type_allows_uninitialized(&mut self, sym_id: SymbolId) -> bool {
        use crate::binder::SymbolId as BinderSymbolId;
        use crate::solver::{LiteralValue, SymbolRef, TypeKey};

        let declared_type = self.get_type_of_symbol(sym_id);

        // TypeScript doesn't check definite assignment for `any` typed variables
        if declared_type == TypeId::ANY {
            return true;
        }

        // Check if it's undefined type
        if declared_type == TypeId::UNDEFINED {
            return true;
        }

        let Some(type_key) = self.ctx.types.lookup(declared_type) else {
            return false;
        };

        // Handle TypeQuery (typeof x) - resolve the underlying type
        if let TypeKey::TypeQuery(SymbolRef(ref_sym_id)) = type_key {
            let resolved = self.get_type_of_symbol(BinderSymbolId(ref_sym_id));
            // Check if resolved type allows uninitialized use
            if resolved == TypeId::UNDEFINED || resolved == TypeId::ANY {
                return true;
            }
            // Also check if the resolved type is a union containing undefined
            if let Some(TypeKey::Union(members)) = self.ctx.types.lookup(resolved) {
                let member_ids = self.ctx.types.type_list(members);
                if member_ids.contains(&TypeId::UNDEFINED) {
                    return true;
                }
            }
        }

        // Check if it's a single literal type
        if matches!(type_key, TypeKey::Literal(_)) {
            return true;
        }

        // Check if it's a union
        if let TypeKey::Union(members) = type_key {
            let member_ids = self.ctx.types.type_list(members);

            // Union of only literal types - allowed without initialization
            let all_literals = member_ids.iter().all(|&member_id| {
                matches!(self.ctx.types.lookup(member_id), Some(TypeKey::Literal(_)))
            });
            if all_literals {
                return true;
            }

            // If union includes undefined, allowed without initialization
            if member_ids.contains(&TypeId::UNDEFINED) {
                return true;
            }
        }

        false
    }

    /// Find the enclosing variable statement for a node.
    fn find_enclosing_variable_statement(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                    return Some(current);
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Check if a variable symbol's declaration has an initializer.
    fn symbol_has_initializer(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(var_decl_idx) = self.find_enclosing_variable_declaration(decl_idx) else {
                continue;
            };
            let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
                continue;
            };
            // Variable has an initializer - it's definitely assigned at declaration
            if !var_decl.initializer.is_none() {
                return true;
            }
        }

        false
    }

    fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
        let flow_node = match self.ctx.binder.get_node_flow(idx) {
            Some(flow) => flow,
            None => return false, // No flow info means variable is not definitely assigned
        };
        let analyzer = FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types);
        analyzer.is_definitely_assigned(idx, flow_node)
    }

    fn symbol_is_parameter(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.node_is_or_within_kind(decl_idx, syntax_kind_ext::PARAMETER))
    }

    fn symbol_has_definite_assignment_assertion(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(var_decl_idx) = self.find_enclosing_variable_declaration(decl_idx) else {
                continue;
            };
            let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
                continue;
            };
            if var_decl.exclamation_token {
                return true;
            }
        }

        false
    }

    fn find_enclosing_variable_declaration(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
    }

    fn node_is_or_within_kind(&self, idx: NodeIndex, kind: u16) -> bool {
        let mut current = idx;
        loop {
            let node = match self.ctx.arena.get(current) {
                Some(node) => node,
                None => return false,
            };
            if node.kind == kind {
                return true;
            }
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
    }

    fn is_for_in_of_assignment_target(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let ext = match self.ctx.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent = ext.parent;
            let parent_node = match self.ctx.arena.get(parent) {
                Some(node) => node,
                None => return false,
            };
            if parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
            {
                if let Some(for_data) = self.ctx.arena.get_for_in_of(parent_node) {
                    let analyzer =
                        FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types);
                    return analyzer.assignment_targets_reference(for_data.initializer, idx);
                }
            }
            current = parent;
        }
    }

    /// Find the enclosing static block for a node, if any.
    ///
    /// Returns the NodeIndex of the CLASS_STATIC_BLOCK_DECLARATION if the node is inside one.
    fn find_enclosing_static_block(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    return Some(current);
                }
                // Stop at function boundaries (don't consider outer static blocks)
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class declaration containing a static block.
    ///
    /// Given a static block node, returns the parent CLASS_DECLARATION or CLASS_EXPRESSION.
    fn find_class_for_static_block(&self, static_block_idx: NodeIndex) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(static_block_idx)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            Some(parent)
        } else {
            None
        }
    }

    /// Check if a variable is used in a static block before its declaration (TDZ check).
    ///
    /// In TypeScript, if a variable is declared at module level AFTER a class declaration,
    /// using that variable inside the class's static block should emit TS2454.
    ///
    /// Example:
    /// ```typescript
    /// class Baz {
    ///     static {
    ///         console.log(FOO);  // Error: Variable 'FOO' is used before being assigned
    ///     }
    /// }
    /// const FOO = "FOO";  // Declared after the class
    /// ```
    fn is_variable_used_before_declaration_in_static_block(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a static block
        let Some(static_block_idx) = self.find_enclosing_static_block(usage_idx) else {
            return false;
        };

        // Get the class containing the static block
        let Some(class_idx) = self.find_class_for_static_block(static_block_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable (not a class member)
        // We're looking for variables declared outside the class
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            // Check if this is a variable declaration
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    /// Find the enclosing computed property name for a node, if any.
    ///
    /// Returns the NodeIndex of the COMPUTED_PROPERTY_NAME if the node is inside one.
    fn find_enclosing_computed_property(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return Some(current);
                }
                // Stop at function boundaries (computed properties inside functions are evaluated at call time)
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class declaration containing a computed property name.
    ///
    /// Walks up from a computed property to find the containing class member,
    /// then finds the class declaration.
    fn find_class_for_computed_property(&self, computed_idx: NodeIndex) -> Option<NodeIndex> {
        // Walk up to find the class member (property, method, accessor)
        let mut current = computed_idx;
        while !current.is_none() {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            // If we found a class, return it
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    /// Check if a variable is used in a computed property name before its declaration (TDZ check).
    ///
    /// In TypeScript, if a variable is declared at module level AFTER a class declaration,
    /// using that variable in a computed property name should emit TS2454.
    ///
    /// Example:
    /// ```typescript
    /// class C {
    ///     [FOO]() {}  // Error: Variable 'FOO' is used before being assigned
    /// }
    /// const FOO = "foo";  // Declared after the class
    /// ```
    fn is_variable_used_before_declaration_in_computed_property(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a computed property name
        let Some(computed_idx) = self.find_enclosing_computed_property(usage_idx) else {
            return false;
        };

        // Get the class containing the computed property
        let Some(class_idx) = self.find_class_for_computed_property(computed_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable (not a class member)
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            // Check if this is a variable declaration
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    /// Find the enclosing heritage clause (extends/implements) for a node, if any.
    ///
    /// Returns the NodeIndex of the HERITAGE_CLAUSE if the node is inside one.
    fn find_enclosing_heritage_clause(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use crate::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == HERITAGE_CLAUSE {
                    return Some(current);
                }
                // Stop at function/class/interface boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class or interface declaration containing a heritage clause.
    fn find_class_for_heritage_clause(&self, heritage_idx: NodeIndex) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(heritage_idx)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
        {
            Some(parent)
        } else {
            None
        }
    }

    /// Check if a variable is used in an extends clause before its declaration (TDZ check).
    ///
    /// Example:
    /// ```typescript
    /// class C extends Base {}  // Error if Base declared after
    /// const Base = class {};
    /// ```
    fn is_variable_used_before_declaration_in_heritage_clause(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        // Check if we're inside a heritage clause
        let Some(heritage_idx) = self.find_enclosing_heritage_clause(usage_idx) else {
            return false;
        };

        // Get the class/interface containing the heritage clause
        let Some(class_idx) = self.find_class_for_heritage_clause(heritage_idx) else {
            return false;
        };

        // Get the class position
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let class_pos = class_node.pos;

        // Get the symbol's declaration
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if the symbol is a module-level variable
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        // Get the position of the variable's declaration
        for &decl_idx in &symbol.declarations {
            let Some(var_stmt_idx) = self.find_enclosing_variable_statement(decl_idx) else {
                continue;
            };
            let Some(var_stmt_node) = self.ctx.arena.get(var_stmt_idx) else {
                continue;
            };

            // Variable is declared AFTER the class - this is TDZ error
            if var_stmt_node.pos > class_pos {
                return true;
            }
        }

        false
    }

    /// Get type of a symbol.
    pub fn get_type_of_symbol(&mut self, sym_id: SymbolId) -> TypeId {
        use crate::solver::SymbolRef;

        self.record_symbol_dependency(sym_id);

        // Check cache first
        if let Some(&cached) = self.ctx.symbol_types.get(&sym_id) {
            return cached;
        }

        // Check for circular reference
        if self.ctx.symbol_resolution_set.contains(&sym_id) {
            return TypeId::ERROR; // Circular reference - propagate error
        }

        // Push onto resolution stack
        self.ctx.symbol_resolution_stack.push(sym_id);
        self.ctx.symbol_resolution_set.insert(sym_id);

        self.push_symbol_dependency(sym_id, true);
        let (result, type_params) = self.compute_type_of_symbol(sym_id);
        self.pop_symbol_dependency();

        // Pop from resolution stack
        self.ctx.symbol_resolution_stack.pop();
        self.ctx.symbol_resolution_set.remove(&sym_id);

        // Cache result
        self.ctx.symbol_types.insert(sym_id, result);

        // Also populate the type environment for Application expansion
        // IMPORTANT: We use the type_params returned by compute_type_of_symbol
        // because those are the same TypeIds used when lowering the type body.
        // Calling get_type_params_for_symbol would create fresh TypeIds that don't match.
        if result != TypeId::ANY && result != TypeId::ERROR {
            let class_env_entry = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                if symbol.flags & symbol_flags::CLASS != 0 {
                    self.class_instance_type_with_params_from_symbol(sym_id)
                } else {
                    None
                }
            });

            let mut env = self.ctx.type_env.borrow_mut();
            if let Some((instance_type, class_params)) = class_env_entry {
                if class_params.is_empty() {
                    env.insert(SymbolRef(sym_id.0), instance_type);
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), instance_type, class_params);
                }
            } else if type_params.is_empty() {
                env.insert(SymbolRef(sym_id.0), result);
            } else {
                env.insert_with_params(SymbolRef(sym_id.0), result, type_params);
            }
        }

        result
    }

    /// Compute type of a symbol (internal, not cached).
    ///
    /// Uses TypeLowering to bridge symbol declarations to solver types.
    /// Returns the computed type and the type parameters used (if any).
    /// IMPORTANT: The type params returned must be the same ones used when lowering
    /// the type body, so that instantiation works correctly.
    fn compute_type_of_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<crate::solver::TypeParamInfo>) {
        use crate::solver::{SymbolRef, TypeKey, TypeLowering};

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return (TypeId::UNKNOWN, Vec::new());
        };

        let flags = symbol.flags;
        let value_decl = symbol.value_declaration;

        // Class - return class constructor type (merging namespace exports when present)
        if flags & symbol_flags::CLASS != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                symbol
                    .declarations
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none() {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(class) = self.ctx.arena.get_class(node) {
                        let ctor_type = self.get_class_constructor_type(decl_idx, class);
                        if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                            != 0
                        {
                            let merged =
                                self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                            return (merged, Vec::new());
                        }
                        return (ctor_type, Vec::new());
                    }
                }
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Namespace / Module
        // Return a Ref type so resolve_qualified_name can access the symbol's exports
        if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
            // Note: We use the symbol ID directly.
            // For merged declarations, this ID points to the unified symbol in the binder.
            return (
                self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0))),
                Vec::new(),
            );
        }

        // Enum - return a nominal reference type
        if flags & symbol_flags::ENUM != 0 {
            return (
                self.ctx.types.intern(TypeKey::Ref(SymbolRef(sym_id.0))),
                Vec::new(),
            );
        }

        // Enum member - determine type from parent enum
        if flags & symbol_flags::ENUM_MEMBER != 0 {
            // Find the parent enum by walking up to find the containing enum declaration
            let member_type = self.enum_member_type_from_decl(value_decl);
            return (member_type, Vec::new());
        }

        // Function - build function type or callable overload set
        if flags & symbol_flags::FUNCTION != 0 {
            use crate::solver::CallableShape;

            let mut overloads = Vec::new();
            let mut implementation_decl = NodeIndex::NONE;

            for &decl_idx in &symbol.declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(func) = self.ctx.arena.get_function(node) else {
                    continue;
                };

                if func.body.is_none() {
                    overloads.push(self.call_signature_from_function(func));
                } else {
                    implementation_decl = decl_idx;
                }
            }

            if !overloads.is_empty() {
                let shape = CallableShape {
                    call_signatures: overloads,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                };
                return (self.ctx.types.callable(shape), Vec::new());
            }

            if !value_decl.is_none() {
                return (self.get_type_of_function(value_decl), Vec::new());
            }
            if !implementation_decl.is_none() {
                return (self.get_type_of_function(implementation_decl), Vec::new());
            }

            return (TypeId::UNKNOWN, Vec::new());
        }

        // Interface - return interface type with call signatures
        if flags & symbol_flags::INTERFACE != 0 {
            if !symbol.declarations.is_empty() {
                // Get type parameters from the first interface declaration
                let mut params = Vec::new();
                let mut updates = Vec::new();

                // Try to get type parameters from the interface declaration
                let first_decl = symbol.declarations.first().copied().unwrap_or(NodeIndex::NONE);
                if !first_decl.is_none() {
                    if let Some(node) = self.ctx.arena.get(first_decl) {
                        if let Some(interface) = self.ctx.arena.get_interface(node) {
                            (params, updates) = self.push_type_parameters(&interface.type_parameters);
                        }
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = TypeLowering::with_resolvers(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let interface_type = lowering.lower_interface_declarations(&symbol.declarations);

                // Restore the type parameter scope
                self.pop_type_parameters(updates);

                // Return the interface type along with the type parameters that were used
                return (
                    self.merge_interface_heritage_types(&symbol.declarations, interface_type),
                    params,
                );
            }
            if !value_decl.is_none() {
                return (self.get_type_of_interface(value_decl), Vec::new());
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Type alias - resolve using checker's get_type_from_type_node to properly resolve symbols
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            // Get the type node from the type alias declaration
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                symbol
                    .declarations
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none() {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                        let (params, updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        let alias_type = self.get_type_from_type_node(type_alias.type_node);
                        self.pop_type_parameters(updates);
                        // Return the params that were used during lowering - this ensures
                        // type_env gets the same TypeIds as the type body
                        return (alias_type, params);
                    }
                }
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Variable - get type from annotation or infer from initializer
        if flags & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            != 0
        {
            if !value_decl.is_none() {
                if let Some(node) = self.ctx.arena.get(value_decl) {
                    if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                        // First try type annotation using type-node lowering (resolves through binder).
                        if !var_decl.type_annotation.is_none() {
                            return (
                                self.get_type_from_type_node(var_decl.type_annotation),
                                Vec::new(),
                            );
                        }
                        if let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(value_decl) {
                            return (jsdoc_type, Vec::new());
                        }
                        if !var_decl.initializer.is_none()
                            && self.is_const_variable_declaration(value_decl)
                        {
                            if let Some(literal_type) =
                                self.literal_type_from_initializer(var_decl.initializer)
                            {
                                return (literal_type, Vec::new());
                            }
                        }
                        // Fall back to inferring from initializer
                        if !var_decl.initializer.is_none() {
                            return (self.get_type_of_node(var_decl.initializer), Vec::new());
                        }
                    }
                }
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Alias - resolve the aliased type (import x = ns.member or ES6 imports)
        if flags & symbol_flags::ALIAS != 0 {
            if !value_decl.is_none() {
                if let Some(node) = self.ctx.arena.get(value_decl) {
                    // Handle Import Equals Declaration (import x = ns.member)
                    if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                        if let Some(import) = self.ctx.arena.get_import_decl(node) {
                            // module_specifier holds the reference (e.g., 'ns.member' or require("..."))
                            // Use resolve_qualified_symbol to get the target symbol directly,
                            // avoiding the value-only check that's inappropriate for import aliases.
                            // Import aliases can legitimately reference value-only namespaces.
                            if let Some(target_sym) =
                                self.resolve_qualified_symbol(import.module_specifier)
                            {
                                return (self.get_type_of_symbol(target_sym), Vec::new());
                            }
                            if let Some(target_sym) =
                                self.resolve_require_call_symbol(import.module_specifier, None)
                            {
                                return (self.get_type_of_symbol(target_sym), Vec::new());
                            }
                            // Check if this is a require() call - if so, return ANY type instead of the literal type
                            // This handles cases like: import x = require('./module') where multi-file module
                            // resolution isn't available. The ANY type allows property access without errors.
                            if self.is_require_call(import.module_specifier) {
                                return (TypeId::ANY, Vec::new());
                            }
                            // Fall back to get_type_of_node for simple identifiers
                            return (self.get_type_of_node(import.module_specifier), Vec::new());
                        }
                    }
                    // Handle ES6 named imports (import { X } from './module')
                    // Use the import_module field to resolve to the actual export
                    // Check if this symbol has import tracking metadata
                }
            }

            // For ES6 imports with import_module set, resolve using module_exports
            if let Some(ref module_name) = symbol.import_module {
                // Use import_name if set (for renamed imports), otherwise use escaped_name
                let export_name = symbol.import_name.as_ref().unwrap_or(&symbol.escaped_name);
                if let Some(exports_table) = self.ctx.binder.module_exports.get(module_name) {
                    if let Some(export_sym_id) = exports_table.get(export_name) {
                        return (self.get_type_of_symbol(export_sym_id), Vec::new());
                    }
                }
                // Module not found in exports - fall through to UNKNOWN
            }

            return (TypeId::UNKNOWN, Vec::new());
        }

        (TypeId::UNKNOWN, Vec::new())
    }

    fn is_const_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        use crate::parser::node_flags;

        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        (parent_node.flags as u32) & node_flags::CONST != 0
    }

    fn is_catch_clause_variable_declaration(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return false;
        }
        let Some(catch) = self.ctx.arena.get_catch_clause(parent_node) else {
            return false;
        };
        catch.variable_declaration == var_decl_idx
    }

    fn literal_type_from_initializer(&self, idx: NodeIndex) -> Option<TypeId> {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(self.ctx.types.literal_string(&lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.map(|value| self.ctx.types.literal_number(value))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(self.ctx.types.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => {
                Some(self.ctx.types.literal_boolean(false))
            }
            k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = unary.operand;
                let Some(operand_node) = self.ctx.arena.get(operand) else {
                    return None;
                };
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand_node)?;
                let value = lit.value?;
                let value = if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                };
                Some(self.ctx.types.literal_number(value))
            }
            _ => None,
        }
    }

    fn contextual_literal_type(&mut self, literal_type: TypeId) -> Option<TypeId> {
        let ctx_type = self.ctx.contextual_type?;
        if self.contextual_type_allows_literal(ctx_type, literal_type) {
            Some(literal_type)
        } else {
            None
        }
    }

    fn contextual_type_allows_literal(&mut self, ctx_type: TypeId, literal_type: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.contextual_type_allows_literal_inner(ctx_type, literal_type, &mut visited)
    }

    fn contextual_type_allows_literal_inner(
        &mut self,
        ctx_type: TypeId,
        literal_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        use crate::solver::TypeKey;

        if ctx_type == literal_type {
            return true;
        }
        if !visited.insert(ctx_type) {
            return false;
        }

        let Some(key) = self.ctx.types.lookup(ctx_type) else {
            return false;
        };

        match key {
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                self.ctx.types.type_list(list_id).iter().any(|&member| {
                    self.contextual_type_allows_literal_inner(member, literal_type, visited)
                })
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => info
                .constraint
                .map(|constraint| {
                    self.contextual_type_allows_literal_inner(constraint, literal_type, visited)
                })
                .unwrap_or(false),
            TypeKey::Ref(symbol) => {
                let resolved = {
                    let env = self.ctx.type_env.borrow();
                    env.get(symbol)
                };
                if let Some(resolved) = resolved {
                    if resolved != ctx_type {
                        return self.contextual_type_allows_literal_inner(
                            resolved,
                            literal_type,
                            visited,
                        );
                    }
                }
                false
            }
            TypeKey::Application(_) => {
                let expanded = self.evaluate_application_type(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            TypeKey::Mapped(_) => {
                let expanded = self.evaluate_mapped_type_with_resolution(ctx_type);
                if expanded != ctx_type {
                    return self.contextual_type_allows_literal_inner(
                        expanded,
                        literal_type,
                        visited,
                    );
                }
                false
            }
            _ => false,
        }
    }

    fn widen_literal_type(&self, type_id: TypeId) -> TypeId {
        use crate::solver::{LiteralValue, TypeKey};

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Literal(literal)) => match literal {
                LiteralValue::String(_) => TypeId::STRING,
                LiteralValue::Number(_) => TypeId::NUMBER,
                LiteralValue::BigInt(_) => TypeId::BIGINT,
                LiteralValue::Boolean(_) => TypeId::BOOLEAN,
            },
            _ => type_id,
        }
    }

    /// Resolve a TypeQuery type to its structural type.
    /// If the type is `typeof x`, this returns the actual type of `x`.
    /// If the type is not a TypeQuery, it returns the type unchanged.
    fn resolve_type_query_to_structural(&mut self, type_id: TypeId) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        if let Some(TypeKey::TypeQuery(SymbolRef(sym_id))) = self.ctx.types.lookup(type_id) {
            // Resolve the symbol to its actual type
            self.get_type_of_symbol(SymbolId(sym_id))
        } else {
            type_id
        }
    }

    /// Get the type of an assignment target without definite assignment checks.
    fn get_type_of_assignment_target(&mut self, idx: NodeIndex) -> TypeId {
        use crate::scanner::SyntaxKind;

        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                    if self.alias_resolves_to_type_only(sym_id) {
                        if let Some(ident) = self.ctx.arena.get_identifier(node) {
                            self.error_type_only_value_at(&ident.escaped_text, idx);
                        }
                        return TypeId::ERROR;
                    }
                    let declared_type = self.get_type_of_symbol(sym_id);
                    return declared_type;
                }
            }
        }

        self.get_type_of_node(idx)
    }

    /// Check an assignment expression, applying contextual typing to the RHS.
    fn check_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        self.check_readonly_assignment(left_idx, expr_idx);

        if left_type != TypeId::ANY {
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    right_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !self.is_assignable_to(right_type, left_type) {
                if !self.should_skip_weak_union_error(right_type, left_type, right_idx) {
                    self.error_type_not_assignable_with_reason_at(right_type, left_type, right_idx);
                }
            }

            if left_type != TypeId::UNKNOWN {
                if let Some(right_node) = self.ctx.arena.get(right_idx) {
                    if right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        self.check_object_literal_excess_properties(
                            right_type, left_type, right_idx,
                        );
                    }
                }
            }
        }

        right_type
    }

    /// Check a compound assignment expression (+=, &&=, ??=, etc.).
    fn check_compound_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        operator: u16,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        self.check_readonly_assignment(left_idx, expr_idx);

        let result_type = self.compound_assignment_result_type(left_type, right_type, operator);
        let is_logical_assignment = matches!(
            operator,
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
        );
        let assigned_type = if is_logical_assignment {
            right_type
        } else {
            result_type
        };

        if left_type != TypeId::ANY {
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    assigned_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !self.is_assignable_to(assigned_type, left_type) {
                if !self.should_skip_weak_union_error(right_type, left_type, right_idx) {
                    self.error_type_not_assignable_with_reason_at(
                        assigned_type,
                        left_type,
                        right_idx,
                    );
                }
            }

            if left_type != TypeId::UNKNOWN {
                if let Some(right_node) = self.ctx.arena.get(right_idx) {
                    if right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        self.check_object_literal_excess_properties(
                            right_type, left_type, right_idx,
                        );
                    }
                }
            }
        }

        result_type
    }

    fn compound_assignment_result_type(
        &self,
        left_type: TypeId,
        right_type: TypeId,
        operator: u16,
    ) -> TypeId {
        use crate::scanner::SyntaxKind;
        use crate::solver::{BinaryOpEvaluator, BinaryOpResult};

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let op_str = match operator {
            k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
            k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::SlashEqualsToken as u16 => Some("/"),
            k if k == SyntaxKind::PercentEqualsToken as u16 => Some("%"),
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => Some("&&"),
            k if k == SyntaxKind::BarBarEqualsToken as u16 => Some("||"),
            _ => None,
        };

        if let Some(op) = op_str {
            return match evaluator.evaluate(left_type, right_type, op) {
                BinaryOpResult::Success(result) => result,
                BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
            };
        }

        if operator == SyntaxKind::QuestionQuestionEqualsToken as u16 {
            return self.ctx.types.union2(left_type, right_type);
        }

        if matches!(
            operator,
            k if k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
        ) {
            return TypeId::NUMBER;
        }

        // Return UNKNOWN instead of ANY for unknown binary operand types
        TypeId::UNKNOWN
    }

    /// Get type of binary expression.
    fn get_type_of_binary_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{BinaryOpEvaluator, BinaryOpResult};

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let mut stack = vec![(idx, false)];
        let mut type_stack: Vec<TypeId> = Vec::new();

        while let Some((node_idx, visited)) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                // Return UNKNOWN instead of ANY when node cannot be found
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                type_stack.push(self.get_type_of_node(node_idx));
                continue;
            }

            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                // Return UNKNOWN instead of ANY when binary expression cannot be extracted
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            let left_idx = binary.left;
            let right_idx = binary.right;
            let op_kind = binary.operator_token;

            if !visited {
                if self.is_assignment_operator(op_kind) {
                    let assign_type = if op_kind == SyntaxKind::EqualsToken as u16 {
                        self.check_assignment_expression(left_idx, right_idx, node_idx)
                    } else {
                        self.check_compound_assignment_expression(
                            left_idx, right_idx, op_kind, node_idx,
                        )
                    };
                    type_stack.push(assign_type);
                    continue;
                }

                stack.push((node_idx, true));
                stack.push((right_idx, false));
                stack.push((left_idx, false));
                continue;
            }

            // Return UNKNOWN instead of ANY when type_stack is empty
            let right_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            let left_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            if op_kind == SyntaxKind::CommaToken as u16 {
                if self.is_side_effect_free(left_idx)
                    && !self.is_indirect_call(node_idx, left_idx, right_idx)
                {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        left_idx,
                        diagnostic_messages::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                        diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                    );
                }
                type_stack.push(right_type);
                continue;
            }
            if op_kind == SyntaxKind::InKeyword as u16 {
                if let Some(left_node) = self.ctx.arena.get(left_idx) {
                    if left_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                        self.check_private_identifier_in_expression(left_idx, right_type);
                    }
                }
                type_stack.push(TypeId::BOOLEAN);
                continue;
            }
            let op_str = match op_kind {
                k if k == SyntaxKind::PlusToken as u16 => "+",
                k if k == SyntaxKind::MinusToken as u16 => "-",
                k if k == SyntaxKind::AsteriskToken as u16 => "*",
                k if k == SyntaxKind::SlashToken as u16 => "/",
                k if k == SyntaxKind::PercentToken as u16 => "%",
                k if k == SyntaxKind::LessThanToken as u16 => "<",
                k if k == SyntaxKind::GreaterThanToken as u16 => ">",
                k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
                k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
                k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
                k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
                k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
                k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
                k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
                k if k == SyntaxKind::BarBarToken as u16 => "||",
                k if k == SyntaxKind::AmpersandToken as u16
                    || k == SyntaxKind::BarToken as u16
                    || k == SyntaxKind::CaretToken as u16
                    || k == SyntaxKind::LessThanLessThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 =>
                {
                    type_stack.push(TypeId::NUMBER);
                    continue;
                }
                _ => {
                    type_stack.push(TypeId::UNKNOWN);
                    continue;
                }
            };

            let result = evaluator.evaluate(left_type, right_type, op_str);
            let result_type = match result {
                BinaryOpResult::Success(result_type) => result_type,
                BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
            };
            type_stack.push(result_type);
        }

        type_stack.pop().unwrap_or(TypeId::UNKNOWN)
    }

    /// Get type of variable declaration.
    fn get_type_of_variable_declaration(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return TypeId::ERROR; // Missing variable declaration data - propagate error
        };

        // First check type annotation - this takes precedence
        if !var_decl.type_annotation.is_none() {
            return self.get_type_from_type_node(var_decl.type_annotation);
        }

        if self.is_catch_clause_variable_declaration(idx) && self.ctx.use_unknown_in_catch_variables
        {
            return TypeId::UNKNOWN;
        }

        // Infer from initializer
        if !var_decl.initializer.is_none() {
            return self.get_type_of_node(var_decl.initializer);
        }

        // No initializer - use UNKNOWN to enforce strict checking
        // This requires explicit type annotation or prevents unsafe usage
        TypeId::UNKNOWN
    }

    fn apply_this_substitution_to_call_return(
        &mut self,
        return_type: TypeId,
        callee_idx: NodeIndex,
    ) -> TypeId {
        if let Some(receiver_type) = self.get_call_receiver_type(callee_idx) {
            return self.substitute_this_type(return_type, receiver_type);
        }
        return_type
    }

    fn get_call_receiver_type(&mut self, callee_idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(callee_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let receiver_type = self.get_type_of_node(access.expression);
        let receiver_type = self.evaluate_application_type(receiver_type);
        let receiver_type = self.resolve_type_for_property_access(receiver_type);
        Some(receiver_type)
    }

    /// Get type of call expression.
    fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::node_flags;
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing call expression data - propagate error
        };

        // Get the type of the callee
        let mut callee_type = self.get_type_of_node(call.expression);

        // Special handling for super() calls - treat as construct call
        let is_super_call = self.is_super_expression(call.expression);

        // Get arguments list (may be None for calls without arguments)
        // IMPORTANT: We must check arguments even if callee is ANY/ERROR to catch definite assignment errors
        let args = call
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            // Still need to check arguments for definite assignment (TS2454) and other errors
            // Create a dummy context helper that returns None for all parameter types
            let ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ANY callee
                check_excess_properties,
            );
            return TypeId::ANY;
        }
        if callee_type == TypeId::ERROR {
            // Still need to check arguments for definite assignment (TS2454) and other errors
            let ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ERROR callee
                check_excess_properties,
            );
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let mut nullish_cause = None;
        if (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0 {
            let (non_nullish, cause) = self.split_nullish_type(callee_type);
            nullish_cause = cause;
            let Some(non_nullish) = non_nullish else {
                return TypeId::UNDEFINED;
            };
            callee_type = non_nullish;
            if callee_type == TypeId::ANY {
                return TypeId::ANY;
            }
            if callee_type == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }
        }

        // args is already defined above before the ANY/ERROR check

        let overload_signatures = match self.ctx.types.lookup(callee_type) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        // Overload candidates need signature-specific contextual typing.
        if let Some(signatures) = overload_signatures.as_deref() {
            if let Some(return_type) =
                self.resolve_overloaded_call_with_signatures(args, signatures)
            {
                let return_type =
                    self.apply_this_substitution_to_call_return(return_type, call.expression);
                return if nullish_cause.is_some() {
                    self.ctx.types.union(vec![return_type, TypeId::UNDEFINED])
                } else {
                    return_type
                };
            }
        }

        // Create contextual context from callee type
        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, callee_type);
        let check_excess_properties = overload_signatures.is_none();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            check_excess_properties,
        );

        // Use CallEvaluator to resolve the call
        self.ensure_application_symbols_resolved(callee_type);
        for &arg_type in &arg_types {
            self.ensure_application_symbols_resolved(arg_type);
        }
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            checker.set_strict_function_types(self.ctx.strict_function_types);
            checker.set_strict_null_checks(self.ctx.strict_null_checks);
            let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
            evaluator.resolve_call(callee_type, &arg_types)
        };

        match result {
            CallResult::Success(return_type) => {
                let return_type =
                    self.apply_this_substitution_to_call_return(return_type, call.expression);
                let return_type =
                    self.refine_mixin_call_return_type(call.expression, &arg_types, return_type);
                if nullish_cause.is_some() {
                    self.ctx.types.union(vec![return_type, TypeId::UNDEFINED])
                } else {
                    return_type
                }
            }

            CallResult::NotCallable { .. } => {
                // Special case: super() calls are valid in constructors and return void
                if is_super_call {
                    return TypeId::VOID;
                }
                // Check if it's specifically a class constructor called without 'new' (TS2348)
                // Only emit TS2348 for types that have construct signatures but zero call signatures
                if self.is_class_constructor_type(callee_type) {
                    self.error_class_constructor_without_new_at(callee_type, call.expression);
                } else {
                    // For other non-callable types, emit the generic not-callable error
                    self.error_not_callable_at(callee_type, call.expression);
                }
                TypeId::ERROR
            }

            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                let expected = expected_max.unwrap_or(expected_min);
                self.error_argument_count_mismatch_at(expected, actual, idx);
                TypeId::ERROR
            }

            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            } => {
                // Report error at the specific argument
                if index < args.len() {
                    let arg_idx = args[index];
                    if !(check_excess_properties
                        && self.should_skip_weak_union_error(actual, expected, arg_idx))
                    {
                        self.error_argument_not_assignable_at(actual, expected, arg_idx);
                    }
                }
                TypeId::ERROR
            }

            CallResult::NoOverloadMatch { failures, .. } => {
                self.error_no_overload_matches_at(idx, &failures);
                TypeId::ERROR
            }
        }
    }

    fn refine_mixin_call_return_type(
        &mut self,
        callee_idx: NodeIndex,
        arg_types: &[TypeId],
        return_type: TypeId,
    ) -> TypeId {
        if return_type == TypeId::ANY || return_type == TypeId::ERROR {
            return return_type;
        }

        let Some(func_decl_idx) = self.function_decl_from_callee(callee_idx) else {
            return return_type;
        };
        let Some(func_node) = self.ctx.arena.get(func_decl_idx) else {
            return return_type;
        };
        let Some(func) = self.ctx.arena.get_function(func_node) else {
            return return_type;
        };
        let Some(class_expr_idx) = self.returned_class_expression(func.body) else {
            return return_type;
        };
        let Some(base_param_index) = self.mixin_base_param_index(class_expr_idx, func) else {
            return return_type;
        };
        let Some(&base_arg_type) = arg_types.get(base_param_index) else {
            return return_type;
        };
        if matches!(base_arg_type, TypeId::ANY | TypeId::ERROR) {
            return return_type;
        }

        let mut refined_return = self.ctx.types.intersection2(return_type, base_arg_type);

        if let Some(base_instance_type) = self.instance_type_from_constructor_type(base_arg_type) {
            refined_return = self
                .merge_base_instance_into_constructor_return(refined_return, base_instance_type);
        }

        let base_props = self.static_properties_from_type(base_arg_type);
        if !base_props.is_empty() {
            refined_return = self.merge_base_constructor_properties_into_constructor_return(
                refined_return,
                &base_props,
            );
        }

        refined_return
    }

    fn function_decl_from_callee(&self, callee_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        if !symbol.value_declaration.is_none() {
            let decl_idx = symbol.value_declaration;
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        None
    }

    fn returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(body_idx)?;
        if node.kind != syntax_kind_ext::BLOCK {
            return self.class_expression_from_expr(body_idx);
        }
        let block = self.ctx.arena.get_block(node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }
            if let Some(expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(expr_idx);
            }
            let expr_node = self.ctx.arena.get(ret.expression)?;
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                if let Some(class_idx) =
                    self.class_declaration_from_identifier_in_block(block, &ident.escaped_text)
                {
                    return Some(class_idx);
                }
            }
        }
        None
    }

    fn class_declaration_from_identifier_in_block(
        &self,
        block: &crate::parser::thin_node::BlockData,
        name: &str,
    ) -> Option<NodeIndex> {
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let class = self.ctx.arena.get_class(stmt)?;
            if class.name.is_none() {
                continue;
            }
            let name_node = self.ctx.arena.get(class.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            if ident.escaped_text == name {
                return Some(stmt_idx);
            }
        }
        None
    }

    fn class_expression_from_expr(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return Some(current);
            }
            return None;
        }
    }

    fn mixin_base_param_index(
        &self,
        class_expr_idx: NodeIndex,
        func: &crate::parser::thin_node::FunctionData,
    ) -> Option<usize> {
        let class_node = self.ctx.arena.get(class_expr_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class_data.heritage_clauses.as_ref()?;

        let mut base_name = None;
        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            let expr_node = self.ctx.arena.get(expr_idx)?;
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let ident = self.ctx.arena.get_identifier(expr_node)?;
            base_name = Some(ident.escaped_text.clone());
            break;
        }

        let base_name = base_name?;
        let mut arg_index = 0usize;
        for &param_idx in &func.parameters.nodes {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            let name_node = self.ctx.arena.get(param.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            if ident.escaped_text == "this" {
                continue;
            }
            if ident.escaped_text == base_name {
                return Some(arg_index);
            }
            arg_index += 1;
        }

        None
    }

    fn instance_type_from_constructor_type(&mut self, ctor_type: TypeId) -> Option<TypeId> {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.instance_type_from_constructor_type_inner(ctor_type, &mut visited)
    }

    fn instance_type_from_constructor_type_inner(
        &mut self,
        ctor_type: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> Option<TypeId> {
        use crate::solver::TypeKey;

        if ctor_type == TypeId::ERROR {
            return None;
        }
        if ctor_type == TypeId::ANY {
            return Some(TypeId::ANY);
        }

        let mut current = ctor_type;
        loop {
            if !visited.insert(current) {
                return None;
            }
            current = self.evaluate_application_type(current);
            let Some(key) = self.ctx.types.lookup(current) else {
                return None;
            };
            match key {
                TypeKey::Callable(shape_id) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    let mut returns = Vec::new();
                    for sig in &shape.construct_signatures {
                        returns.push(sig.return_type);
                    }
                    if returns.is_empty() {
                        return None;
                    }
                    let instance_type = if returns.len() == 1 {
                        returns[0]
                    } else {
                        self.ctx.types.union(returns)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                TypeKey::Function(shape_id) => {
                    let shape = self.ctx.types.function_shape(shape_id);
                    if !shape.is_constructor {
                        return None;
                    }
                    return Some(self.resolve_type_for_property_access(shape.return_type));
                }
                TypeKey::Intersection(members_id) => {
                    let members = self.ctx.types.type_list(members_id);
                    let mut instance_types = Vec::new();
                    for &member in members.iter() {
                        if let Some(instance_type) =
                            self.instance_type_from_constructor_type_inner(member, visited)
                        {
                            instance_types.push(instance_type);
                        }
                    }
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type = if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.ctx.types.intersection(instance_types)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                TypeKey::Union(members_id) => {
                    let members = self.ctx.types.type_list(members_id);
                    let mut instance_types = Vec::new();
                    for &member in members.iter() {
                        if let Some(instance_type) =
                            self.instance_type_from_constructor_type_inner(member, visited)
                        {
                            instance_types.push(instance_type);
                        }
                    }
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type = if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.ctx.types.union(instance_types)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                TypeKey::ReadonlyType(inner) => {
                    return self.instance_type_from_constructor_type_inner(inner, visited);
                }
                TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                    let Some(constraint) = info.constraint else {
                        return None;
                    };
                    current = constraint;
                }
                TypeKey::Conditional(_)
                | TypeKey::Mapped(_)
                | TypeKey::IndexAccess(_, _)
                | TypeKey::KeyOf(_) => {
                    let evaluated = self.evaluate_type_with_env(current);
                    if evaluated == current {
                        return None;
                    }
                    current = evaluated;
                }
                _ => return None,
            }
        }
    }

    fn merge_base_instance_into_constructor_return(
        &mut self,
        ctor_type: TypeId,
        base_instance_type: TypeId,
    ) -> TypeId {
        use crate::solver::TypeKey;

        let Some(key) = self.ctx.types.lookup(ctor_type) else {
            return ctor_type;
        };

        match key {
            TypeKey::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    return ctor_type;
                }
                let mut new_shape = (*shape).clone();
                new_shape.construct_signatures = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| {
                        let mut updated = sig.clone();
                        updated.return_type = self
                            .ctx
                            .types
                            .intersection2(updated.return_type, base_instance_type);
                        updated
                    })
                    .collect();
                self.ctx.types.callable(new_shape)
            }
            TypeKey::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if !shape.is_constructor {
                    return ctor_type;
                }
                let mut new_shape = (*shape).clone();
                new_shape.return_type = self
                    .ctx
                    .types
                    .intersection2(new_shape.return_type, base_instance_type);
                self.ctx.types.function(new_shape)
            }
            TypeKey::Intersection(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let mut updated_members = Vec::with_capacity(members.len());
                let mut changed = false;
                for &member in members.iter() {
                    let updated = self
                        .merge_base_instance_into_constructor_return(member, base_instance_type);
                    if updated != member {
                        changed = true;
                    }
                    updated_members.push(updated);
                }
                if changed {
                    self.ctx.types.intersection(updated_members)
                } else {
                    ctor_type
                }
            }
            _ => ctor_type,
        }
    }

    fn merge_base_constructor_properties_into_constructor_return(
        &mut self,
        ctor_type: TypeId,
        base_props: &rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
    ) -> TypeId {
        use crate::solver::TypeKey;
        use rustc_hash::FxHashMap;

        if base_props.is_empty() {
            return ctor_type;
        }

        let Some(key) = self.ctx.types.lookup(ctor_type) else {
            return ctor_type;
        };

        match key {
            TypeKey::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let mut prop_map: FxHashMap<Atom, crate::solver::PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| (prop.name, prop.clone()))
                    .collect();
                for (name, prop) in base_props.iter() {
                    prop_map.entry(*name).or_insert_with(|| prop.clone());
                }
                let mut new_shape = (*shape).clone();
                new_shape.properties = prop_map.into_values().collect();
                self.ctx.types.callable(new_shape)
            }
            TypeKey::Intersection(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let mut updated_members = Vec::with_capacity(members.len());
                let mut changed = false;
                for &member in members.iter() {
                    let updated = self.merge_base_constructor_properties_into_constructor_return(
                        member, base_props,
                    );
                    if updated != member {
                        changed = true;
                    }
                    updated_members.push(updated);
                }
                if changed {
                    self.ctx.types.intersection(updated_members)
                } else {
                    ctor_type
                }
            }
            _ => ctor_type,
        }
    }

    fn collect_call_argument_types_with_context<F>(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: F,
        check_excess_properties: bool,
    ) -> Vec<TypeId>
    where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        use crate::solver::TypeKey;

        // First pass: count expanded arguments (spreads of tuple types expand to multiple args)
        let mut expanded_count = 0usize;
        for &arg_idx in args.iter() {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    if let Some(spread_data) = self.ctx.arena.get_spread(arg_node) {
                        let spread_type = self.get_type_of_node(spread_data.expression);
                        let spread_type = self.resolve_type_for_property_access(spread_type);
                        if let Some(TypeKey::Tuple(elems_id)) = self.ctx.types.lookup(spread_type) {
                            let elems = self.ctx.types.tuple_list(elems_id);
                            expanded_count += elems.len();
                            continue;
                        }
                    }
                }
            }
            expanded_count += 1;
        }

        let mut arg_types = Vec::with_capacity(expanded_count);
        let mut effective_index = 0usize;

        for &arg_idx in args.iter() {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                // Handle spread elements specially - expand tuple types
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    if let Some(spread_data) = self.ctx.arena.get_spread(arg_node) {
                        let spread_type = self.get_type_of_node(spread_data.expression);
                        let spread_type = self.resolve_type_for_property_access(spread_type);

                        // If it's a tuple type, expand its elements
                        if let Some(TypeKey::Tuple(elems_id)) = self.ctx.types.lookup(spread_type) {
                            let elems = self.ctx.types.tuple_list(elems_id);
                            for elem in elems.iter() {
                                arg_types.push(elem.type_id);
                                effective_index += 1;
                            }
                            continue;
                        }

                        // If it's an array type, push the element type (variadic handling)
                        if let Some(TypeKey::Array(elem_type)) = self.ctx.types.lookup(spread_type)
                        {
                            arg_types.push(elem_type);
                            effective_index += 1;
                            continue;
                        }

                        // Otherwise just push the spread type as-is
                        arg_types.push(spread_type);
                        effective_index += 1;
                        continue;
                    }
                }
            }

            // Regular (non-spread) argument
            let expected_type = expected_for_index(effective_index, expanded_count);

            let prev_context = self.ctx.contextual_type;
            self.ctx.contextual_type = expected_type;

            let arg_type = self.get_type_of_node(arg_idx);
            arg_types.push(arg_type);

            if check_excess_properties {
                if let Some(expected) = expected_type {
                    if expected != TypeId::ANY && expected != TypeId::UNKNOWN {
                        if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                            if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                                self.check_object_literal_excess_properties(
                                    arg_type, expected, arg_idx,
                                );
                            }
                        }
                    }
                }
            }

            self.ctx.contextual_type = prev_context;
            effective_index += 1;
        }

        arg_types
    }

    fn check_call_argument_excess_properties<F>(
        &mut self,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        mut expected_for_index: F,
    ) where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        let arg_count = args.len();
        for (i, &arg_idx) in args.iter().enumerate() {
            let expected = expected_for_index(i, arg_count);
            if let Some(expected) = expected {
                if expected != TypeId::ANY && expected != TypeId::UNKNOWN {
                    if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                        if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                            let arg_type = arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN);
                            self.check_object_literal_excess_properties(
                                arg_type, expected, arg_idx,
                            );
                        }
                    }
                }
            }
        }
    }

    fn resolve_overloaded_call_with_signatures(
        &mut self,
        args: &[NodeIndex],
        signatures: &[crate::solver::CallSignature],
    ) -> Option<TypeId> {
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, FunctionShape};

        if signatures.is_empty() {
            return None;
        }

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        for sig in signatures {
            let func_shape = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate.clone(),
                is_constructor: false,
                is_method: false,
            };
            let func_type = self.ctx.types.function(func_shape);
            let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, func_type);

            self.ctx.node_types = Default::default();
            let arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                false,
            );
            let temp_node_types = std::mem::take(&mut self.ctx.node_types);

            self.ctx.node_types = std::mem::take(&mut original_node_types);
            self.ensure_application_symbols_resolved(func_type);
            for &arg_type in &arg_types {
                self.ensure_application_symbols_resolved(arg_type);
            }
            let result = {
                let env = self.ctx.type_env.borrow();
                let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
                checker.set_strict_null_checks(self.ctx.strict_null_checks);
                let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
                evaluator.resolve_call(func_type, &arg_types)
            };

            if let CallResult::Success(return_type) = result {
                self.ctx.node_types.extend(temp_node_types);
                self.check_call_argument_excess_properties(args, &arg_types, |i, arg_count| {
                    ctx_helper.get_parameter_type_for_call(i, arg_count)
                });
                return Some(return_type);
            }

            original_node_types = std::mem::take(&mut self.ctx.node_types);
        }

        self.ctx.node_types = original_node_types;
        None
    }

    /// Get type of new expression.
    fn get_type_of_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::solver::{CallEvaluator, CallResult, CallableShape, CompatChecker, TypeKey};
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(new_expr) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing new expression data - propagate error
        };

        // Check if trying to instantiate an abstract class
        // The expression is typically an identifier referencing the class
        if let Some(expr_node) = self.ctx.arena.get(new_expr.expression) {
            // If it's a direct identifier (e.g., `new MyClass()`)
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                let class_name = &ident.escaped_text;

                // Try multiple ways to find the symbol:
                // 1. Check if the identifier node has a direct symbol binding
                // 2. Look up in file_locals
                // 3. Search all symbols by name (handles local scopes like classes inside functions)

                let symbol_opt = self
                    .ctx
                    .binder
                    .get_node_symbol(new_expr.expression)
                    .or_else(|| self.ctx.binder.file_locals.get(class_name))
                    .or_else(|| self.ctx.binder.get_symbols().find_by_name(class_name));

                if let Some(sym_id) = symbol_opt {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        // Check if it has the ABSTRACT flag
                        if symbol.flags & symbol_flags::ABSTRACT != 0 {
                            self.error_at_node(
                                idx,
                                "Cannot create an instance of an abstract class.",
                                diagnostic_codes::CANNOT_CREATE_INSTANCE_OF_ABSTRACT_CLASS,
                            );
                            return TypeId::ERROR;
                        }
                    }
                }
            }
        }

        // Get the type of the constructor expression
        let constructor_type = self.get_type_of_node(new_expr.expression);

        // Check if the constructor type contains any abstract classes (for union types)
        // e.g., `new cls()` where `cls: typeof AbstractA | typeof AbstractB`
        if self.type_contains_abstract_class(constructor_type) {
            self.error_at_node(
                idx,
                "Cannot create an instance of an abstract class.",
                diagnostic_codes::CANNOT_CREATE_INSTANCE_OF_ABSTRACT_CLASS,
            );
            return TypeId::ERROR;
        }

        if constructor_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if constructor_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let construct_type = match self.ctx.types.lookup(constructor_type) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    None
                } else {
                    Some(self.ctx.types.callable(CallableShape {
                        call_signatures: shape.construct_signatures.clone(),
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                    }))
                }
            }
            Some(TypeKey::Function(_)) => Some(constructor_type),
            Some(TypeKey::Intersection(members_id)) => {
                // For intersection of constructors (mixin pattern), the result is an
                // intersection of all instance types. Handle this specially.
                let members = self.ctx.types.type_list(members_id);
                let mut instance_types: Vec<TypeId> = Vec::new();

                for &member in members.iter() {
                    if let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(member) {
                        let shape = self.ctx.types.callable_shape(shape_id);
                        // Get the return type from the first construct signature
                        if let Some(sig) = shape.construct_signatures.first() {
                            instance_types.push(sig.return_type);
                        }
                    }
                }

                if instance_types.is_empty() {
                    return TypeId::ERROR; // No construct signatures in intersection - expose error
                } else if instance_types.len() == 1 {
                    return instance_types[0];
                } else {
                    // Return intersection of all instance types
                    return self.ctx.types.intersection(instance_types);
                }
            }
            _ => None,
        };

        let Some(construct_type) = construct_type else {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        };

        let args = new_expr
            .arguments
            .as_ref()
            .map(|a| &a.nodes)
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        let overload_signatures = match self.ctx.types.lookup(construct_type) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.call_signatures.len() > 1 {
                    Some(shape.call_signatures.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(signatures) = overload_signatures.as_deref() {
            if let Some(return_type) =
                self.resolve_overloaded_call_with_signatures(args, signatures)
            {
                return return_type;
            }
        }

        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, construct_type);
        let check_excess_properties = overload_signatures.is_none();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            check_excess_properties,
        );

        self.ensure_application_symbols_resolved(construct_type);
        for &arg_type in &arg_types {
            self.ensure_application_symbols_resolved(arg_type);
        }
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
            checker.set_strict_null_checks(self.ctx.strict_null_checks);
            let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
            evaluator.resolve_call(construct_type, &arg_types)
        };

        match result {
            CallResult::Success(return_type) => return_type,
            CallResult::NotCallable { .. } => {
                self.error_not_callable_at(constructor_type, new_expr.expression);
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                let expected = expected_max.unwrap_or(expected_min);
                self.error_argument_count_mismatch_at(expected, actual, idx);
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            } => {
                if index < args.len() {
                    let arg_idx = args[index];
                    if !(check_excess_properties
                        && self.should_skip_weak_union_error(actual, expected, arg_idx))
                    {
                        self.error_argument_not_assignable_at(actual, expected, arg_idx);
                    }
                }
                TypeId::ERROR
            }
            CallResult::NoOverloadMatch { failures, .. } => {
                self.error_no_overload_matches_at(idx, &failures);
                TypeId::ERROR
            }
        }
    }

    /// Check if a type contains any abstract class constructors.
    /// This handles union types like `typeof AbstractA | typeof ConcreteB`.
    fn type_contains_abstract_class(&self, type_id: TypeId) -> bool {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        let Some(type_key) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        match type_key {
            // TypeQuery is `typeof ClassName` - check if the symbol is abstract
            // Since get_type_from_type_query now uses real SymbolIds, we can directly look up
            TypeKey::TypeQuery(SymbolRef(sym_id)) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                    if symbol.flags & symbol_flags::ABSTRACT != 0 {
                        return true;
                    }
                }
                false
            }
            // Union type - check if ANY constituent is abstract
            TypeKey::Union(members) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_abstract_class(member))
            }
            // Intersection type - check if ANY constituent is abstract
            TypeKey::Intersection(members) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_abstract_class(member))
            }
            _ => false,
        }
    }

    fn enum_member_type_for_name(&self, sym_id: SymbolId, property_name: &str) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let member_type = match self.enum_kind(sym_id) {
            Some(EnumKind::String) => TypeId::STRING,
            Some(EnumKind::Numeric) => TypeId::NUMBER,
            None => {
                // Return UNKNOWN instead of ANY for enum without explicit kind
                TypeId::UNKNOWN
            }
        };

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                if let Some(name) = self.get_property_name(member.name) {
                    if name == property_name {
                        return Some(member_type);
                    }
                }
            }
        }

        None
    }

    fn resolve_namespace_value_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(object_type) else {
            return None;
        };

        let symbol = self.ctx.binder.get_symbol(SymbolId(sym_id))?;
        if symbol.flags & (symbol_flags::MODULE | symbol_flags::ENUM) == 0 {
            return None;
        }

        if let Some(exports) = symbol.exports.as_ref() {
            if let Some(member_id) = exports.get(property_name) {
                if let Some(member_symbol) = self.ctx.binder.get_symbol(member_id) {
                    if member_symbol.flags & symbol_flags::VALUE == 0
                        && member_symbol.flags & symbol_flags::ALIAS == 0
                    {
                        return None;
                    }
                }
                return Some(self.get_type_of_symbol(member_id));
            }
        }

        if symbol.flags & symbol_flags::ENUM != 0 {
            if let Some(member_type) =
                self.enum_member_type_for_name(SymbolId(sym_id), property_name)
            {
                return Some(member_type);
            }
        }

        None
    }

    fn namespace_has_type_only_member(&self, object_type: TypeId, property_name: &str) -> bool {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(object_type) else {
            return false;
        };

        let symbol = match self.ctx.binder.get_symbol(SymbolId(sym_id)) {
            Some(symbol) => symbol,
            None => return false,
        };

        if symbol.flags & symbol_flags::MODULE == 0 {
            return false;
        }

        let exports = match symbol.exports.as_ref() {
            Some(exports) => exports,
            None => return false,
        };

        let member_id = match exports.get(property_name) {
            Some(member_id) => member_id,
            None => return false,
        };

        let member_symbol = match self.ctx.binder.get_symbol(member_id) {
            Some(member_symbol) => member_symbol,
            None => return false,
        };

        let has_value = (member_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0;
        let has_type = (member_symbol.flags & symbol_flags::TYPE) != 0;
        has_type && !has_value
    }

    fn alias_resolves_to_type_only(&self, sym_id: SymbolId) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(symbol) => symbol,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }
        if symbol.is_type_only {
            return true;
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        let target_symbol = match self.ctx.binder.get_symbol(target) {
            Some(target_symbol) => target_symbol,
            None => return false,
        };

        let has_value = (target_symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (target_symbol.flags & symbol_flags::TYPE) != 0;
        has_type && !has_value
    }

    fn symbol_is_value_only(&self, sym_id: SymbolId) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(symbol) => symbol,
            None => return false,
        };

        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        has_value && !has_type
    }

    fn alias_resolves_to_value_only(&self, sym_id: SymbolId) -> bool {
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(symbol) => symbol,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        self.symbol_is_value_only(target)
    }

    /// Get type of property access expression.
    fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        *self.ctx.instantiation_depth.borrow_mut() += 1;
        let result = self.get_type_of_property_access_inner(idx);
        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        result
    }

    fn get_type_of_property_access_inner(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return TypeId::ERROR; // Missing name node - propagate error
        };
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            if ident.escaped_text.is_empty() {
                return TypeId::ERROR; // Empty identifier - propagate error
            }
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression) {
                if let Some(ref class_info) = self.ctx.enclosing_class.clone() {
                    if class_info.in_constructor
                        && self.is_abstract_member(&class_info.member_nodes, property_name)
                    {
                        self.error_abstract_property_in_constructor(
                            property_name,
                            &class_info.name,
                            access.name_or_argument,
                        );
                    }
                }
            }
        }

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);

        // Evaluate Application types to resolve generic type aliases/interfaces
        let object_type = self.evaluate_application_type(object_type);

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
            );
        }

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if self.is_global_this_expression(access.expression) {
                let property_type =
                    self.resolve_global_this_property_type(property_name, access.name_or_argument);
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                return self.apply_flow_narrowing(idx, property_type);
            }
        }

        // Enforce private/protected access modifiers when possible
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if !self.check_property_accessibility(
                access.expression,
                property_name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        }

        // Don't report errors for any/error types
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Check for merged class/enum/function + namespace symbols
        // When a class/enum/function merges with a namespace (same name), the symbol has both
        // value constructor flags and MODULE flags. We need to check the symbol's exports.
        // This handles value access like `Foo.value` when Foo is both a class and namespace.
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            // For value access to merged symbols, check the exports directly
            // This is needed because the type system doesn't track which symbol a Callable came from
            if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                if let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node) {
                    let expr_name = &expr_ident.escaped_text;
                    // Try file_locals first (fast path for top-level symbols)
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name) {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            // Check if this is a merged symbol (has both MODULE and value constructor flags)
                            let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                                && (symbol.flags
                                    & (symbol_flags::CLASS
                                        | symbol_flags::FUNCTION
                                        | symbol_flags::REGULAR_ENUM))
                                    != 0;

                            if is_merged {
                                if let Some(exports) = symbol.exports.as_ref() {
                                    if let Some(member_id) = exports.get(property_name) {
                                        // For merged symbols, we return the type for any exported member
                                        let member_type = self.get_type_of_symbol(member_id);
                                        return self.apply_flow_narrowing(idx, member_type);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type, property_name)
            {
                return self.apply_flow_narrowing(idx, member_type);
            }
            if self.namespace_has_type_only_member(object_type, property_name) {
                self.error_type_only_value_at(property_name, access.name_or_argument);
                return TypeId::ERROR;
            }

            let object_type_for_access = self.resolve_type_for_property_access(object_type);
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            // Use solver QueryDatabase to resolve the property access
            let result = self
                .ctx
                .types
                .property_access_type(object_type_for_access, property_name);

            match result {
                PropertyAccessResult::Success {
                    type_id: prop_type,
                    from_index_signature,
                } => {
                    // Check for error 4111: property access from index signature
                    if from_index_signature {
                        use crate::checker::types::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{}' comes from an index signature, so it must be accessed with ['{}'].",
                                property_name, property_name
                            ),
                            diagnostic_codes::PROPERTY_ACCESS_FROM_INDEX_SIGNATURE,
                        );
                    }
                    self.apply_flow_narrowing(idx, prop_type)
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    // Check for optional chaining (?.) - suppress TS2339 error when using optional chaining
                    if access.question_dot_token {
                        // With optional chaining, missing property results in undefined
                        return TypeId::UNDEFINED;
                    }
                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                    if !property_name.starts_with('#') {
                        // Callable types (functions) allow arbitrary property access
                        // because functions are objects at runtime and can have additional properties
                        if !self.is_callable_type(object_type_for_access) {
                            self.error_property_not_exist_at(
                                property_name,
                                object_type_for_access,
                                idx,
                            );
                        }
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    // Check for optional chaining (?.)
                    if access.question_dot_token {
                        // Suppress error, return (property_type | undefined)
                        let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
                        return self.ctx.types.union(vec![base_type, TypeId::UNDEFINED]);
                    }

                    // Report error based on the cause
                    use crate::checker::types::diagnostics::diagnostic_codes;

                    let (code, message) = if cause == TypeId::NULL {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                            "Object is possibly 'null'.",
                        )
                    } else if cause == TypeId::UNDEFINED {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                            "Object is possibly 'undefined'.",
                        )
                    } else {
                        (
                            diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                            "Object is possibly 'null' or 'undefined'.",
                        )
                    };

                    // Report the error on the expression part
                    self.error_at_node(access.expression, message, code);

                    // Error recovery: return the property type found in valid members
                    self.apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR))
                }

                PropertyAccessResult::IsUnknown => {
                    // TS2571: Object is of type 'unknown'
                    // Unknown requires explicit type narrowing before property access
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        access.expression,
                        "Object is of type 'unknown'.",
                        diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                    );
                    TypeId::ERROR
                }
            }
        } else {
            TypeId::ANY
        }
    }

    /// Get the type of a property access when we know the property name.
    /// This is used for private member access when symbols resolution fails
    /// but the property exists in the object type.
    fn get_type_of_property_access_by_name(
        &mut self,
        idx: NodeIndex,
        access: &crate::parser::thin_node::AccessExprData,
        object_type: TypeId,
        property_name: &str,
    ) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let object_type = self.resolve_type_for_property_access(object_type);
        let result_type = match self
            .ctx
            .types
            .property_access_type(object_type, property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
            } => {
                if from_index_signature {
                    self.error_property_not_exist_at(&property_name.to_string(), object_type, idx);
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                if !property_name.starts_with('#') {
                    self.error_property_not_exist_at(&property_name.to_string(), object_type, idx);
                }
                TypeId::ERROR
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2571: Object is of type 'unknown'
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.error_at_node(
                    access.expression,
                    "Object is of type 'unknown'.",
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
                TypeId::ERROR
            }
        };

        // Handle nullish coercion
        if access.question_dot_token {
            self.ctx.types.union(vec![result_type, TypeId::UNDEFINED])
        } else {
            result_type
        }
    }

    fn get_type_of_private_property_access(
        &mut self,
        idx: NodeIndex,
        access: &crate::parser::thin_node::AccessExprData,
        name_idx: NodeIndex,
        object_type: TypeId,
    ) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);

        let object_type = self.evaluate_application_type(object_type);
        let (object_type_for_check, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_check) = object_type_for_check else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                self.report_possibly_nullish_object(access.expression, cause);
            }
            return TypeId::ERROR;
        };

        // When symbols are empty but we're inside a class scope, check if the object type
        // itself has private properties matching the name. This handles cases like:
        //   let a: A2 = this;
        //   a.#prop;  // Should work if A2 has #prop
        if symbols.is_empty() {
            // Try to find the property directly in the object type
            use crate::solver::{PropertyAccessResult, QueryDatabase, TypeKey};
            match self
                .ctx
                .types
                .property_access_type(object_type_for_check, &property_name)
            {
                PropertyAccessResult::Success { .. } => {
                    // Property exists in the type, proceed with the access
                    return self.get_type_of_property_access_by_name(
                        idx,
                        access,
                        object_type_for_check,
                        &property_name,
                    );
                }
                _ => {
                    // FALLBACK: Manually check if the property exists in the callable type
                    // This fixes cases where property_access_type fails due to atom comparison issues
                    // The property IS in the type (as shown by error messages), but the lookup fails
                    //
                    // Important: We need to resolve TypeKey::Ref to get the actual Callable type
                    let resolved_type =
                        self.resolve_type_for_property_access(object_type_for_check);
                    if let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(resolved_type)
                    {
                        let shape = self.ctx.types.callable_shape(shape_id);
                        let prop_atom = self.ctx.types.intern_string(&property_name);
                        for prop in &shape.properties {
                            if prop.name == prop_atom {
                                // Property found in the callable's properties list!
                                // Return the property type (handle optional and write_type)
                                let prop_type = if prop.optional {
                                    self.ctx.types.union(vec![prop.type_id, TypeId::UNDEFINED])
                                } else {
                                    prop.type_id
                                };
                                return self.apply_flow_narrowing(idx, prop_type);
                            }
                        }
                    }

                    // Property not found, emit error if appropriate
                    if saw_class_scope {
                        self.error_property_not_exist_at(&property_name, object_type, name_idx);
                    }
                    return TypeId::ERROR;
                }
            }
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_check,
                        name_idx,
                    );
                }
                return TypeId::ERROR;
            }
        };

        if object_type_for_check == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type_for_check == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }
        if object_type_for_check == TypeId::UNKNOWN {
            return TypeId::ANY; // UNKNOWN remains ANY for now (could be stricter)
        }

        // For private member access, use nominal typing based on private brand.
        // If both types have the same private brand, they're from the same class
        // declaration and the access should be allowed.
        let types_compatible =
            if self.types_have_same_private_brand(object_type_for_check, declaring_type) {
                true
            } else {
                self.is_assignable_to(object_type_for_check, declaring_type)
            };

        if !types_compatible {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .map(|ty| {
                        if self.types_have_same_private_brand(object_type_for_check, ty) {
                            true
                        } else {
                            self.is_assignable_to(object_type_for_check, ty)
                        }
                    })
                    .unwrap_or(false)
            });
            if shadowed {
                return TypeId::ANY;
            }

            self.error_property_not_exist_at(&property_name, object_type_for_check, name_idx);
            return TypeId::ERROR;
        }

        let declaring_type = self.resolve_type_for_property_access(declaring_type);
        let mut result_type = match self
            .ctx
            .types
            .property_access_type(declaring_type, &property_name)
        {
            PropertyAccessResult::Success {
                type_id,
                from_index_signature,
            } => {
                if from_index_signature {
                    // Private fields can't come from index signatures
                    self.error_property_not_exist_at(
                        &property_name,
                        object_type_for_check,
                        name_idx,
                    );
                    return TypeId::ERROR;
                }
                type_id
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If we got here, we already resolved the symbol, so the private field exists.
                // The solver might not find it due to type encoding issues.
                // FALLBACK: Try to manually find the property in the callable type
                use crate::solver::TypeKey;
                if let Some(TypeKey::Callable(shape_id)) = self.ctx.types.lookup(declaring_type) {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    let prop_atom = self.ctx.types.intern_string(&property_name);
                    for prop in &shape.properties {
                        if prop.name == prop_atom {
                            // Property found! Return its type
                            return if prop.optional {
                                self.ctx.types.union(vec![prop.type_id, TypeId::UNDEFINED])
                            } else {
                                prop.type_id
                            };
                        }
                    }
                }
                // Property not found even in fallback, return ANY for type recovery
                TypeId::ANY
            }
            PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                property_type.unwrap_or(TypeId::UNKNOWN)
            }
            PropertyAccessResult::IsUnknown => {
                // TS2571: Object is of type 'unknown'
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.error_at_node(
                    access.expression,
                    "Object is of type 'unknown'.",
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
                TypeId::ERROR
            }
        };

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self.ctx.types.union(vec![result_type, TypeId::UNDEFINED]);
            } else {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    fn check_private_identifier_in_expression(&mut self, name_idx: NodeIndex, rhs_type: TypeId) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let property_name = ident.escaped_text.clone();

        let (symbols, saw_class_scope) = self.resolve_private_identifier_symbols(name_idx);
        if symbols.is_empty() {
            if saw_class_scope {
                self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
            }
            return;
        }

        let rhs_type = self.evaluate_application_type(rhs_type);
        if rhs_type == TypeId::ANY || rhs_type == TypeId::ERROR || rhs_type == TypeId::UNKNOWN {
            return;
        }

        let declaring_type = match self.private_member_declaring_type(symbols[0]) {
            Some(ty) => ty,
            None => {
                if saw_class_scope {
                    self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
                }
                return;
            }
        };

        if !self.is_assignable_to(rhs_type, declaring_type) {
            let shadowed = symbols.iter().skip(1).any(|sym_id| {
                self.private_member_declaring_type(*sym_id)
                    .map(|ty| self.is_assignable_to(rhs_type, ty))
                    .unwrap_or(false)
            });
            if shadowed {
                return;
            }

            self.error_property_not_exist_at(&property_name, rhs_type, name_idx);
        }
    }

    /// Get type of element access expression (e.g., arr[0], obj["prop"]).
    fn get_type_of_element_access(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };

        // Get the type of the object
        let object_type = self.get_type_of_node(access.expression);
        let object_type = self.evaluate_application_type(object_type);

        let literal_string = self.get_literal_string_from_node(access.name_or_argument);
        let numeric_string_index = literal_string
            .as_deref()
            .and_then(|name| self.get_numeric_index_from_string(name));
        let literal_index = self
            .get_literal_index_from_node(access.name_or_argument)
            .or(numeric_string_index);

        if let Some(name) = literal_string.as_deref() {
            if self.is_global_this_expression(access.expression) {
                let property_type =
                    self.resolve_global_this_property_type(name, access.name_or_argument);
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                return self.apply_flow_narrowing(idx, property_type);
            }
        }

        if let Some(name) = literal_string.as_deref() {
            if !self.check_property_accessibility(
                access.expression,
                name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        } else if let Some(index) = literal_index {
            let name = index.to_string();
            if !self.check_property_accessibility(
                access.expression,
                &name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        }

        // Don't report errors for any/error types
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let object_type = self.resolve_type_for_property_access(object_type);
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
        let Some(object_type_for_access) = object_type_for_access else {
            if access.question_dot_token {
                return TypeId::UNDEFINED;
            }
            if let Some(cause) = nullish_cause {
                self.report_possibly_nullish_object(access.expression, cause);
            }
            return TypeId::ERROR;
        };

        let index_type = self.get_type_of_node(access.name_or_argument);
        let literal_string_is_none = literal_string.is_none();

        let mut result_type = None;
        let mut report_no_index = false;
        let mut use_index_signature_check = true;

        if let Some(name) = literal_string.as_deref() {
            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type_for_access, name)
            {
                result_type = Some(member_type);
                use_index_signature_check = false;
            } else if self.namespace_has_type_only_member(object_type_for_access, name) {
                self.error_type_only_value_at(name, access.name_or_argument);
                return TypeId::ERROR;
            }
        }

        if result_type.is_none() && literal_index.is_none() {
            if let Some((string_keys, number_keys)) =
                self.get_literal_key_union_from_type(index_type)
            {
                let total_keys = string_keys.len() + number_keys.len();
                if total_keys > 1 || literal_string_is_none {
                    if !string_keys.is_empty() && number_keys.is_empty() {
                        use_index_signature_check = false;
                    }

                    let mut types = Vec::new();
                    if !string_keys.is_empty() {
                        match self.get_element_access_type_for_literal_keys(
                            object_type_for_access,
                            &string_keys,
                        ) {
                            Some(result) => types.push(result),
                            None => report_no_index = true,
                        }
                    }

                    if !number_keys.is_empty() {
                        match self.get_element_access_type_for_literal_number_keys(
                            object_type_for_access,
                            &number_keys,
                        ) {
                            Some(result) => types.push(result),
                            None => report_no_index = true,
                        }
                    }

                    if report_no_index {
                        result_type = Some(TypeId::ANY);
                    } else if !types.is_empty() {
                        result_type = Some(if types.len() == 1 {
                            types[0]
                        } else {
                            self.ctx.types.union(types)
                        });
                    }
                }
            }
        }

        if result_type.is_none() {
            if let Some(property_name) = self.get_literal_string_from_node(access.name_or_argument)
            {
                if numeric_string_index.is_none() {
                    use_index_signature_check = false;
                    let result = self
                        .ctx
                        .types
                        .property_access_type(object_type_for_access, &property_name);
                    result_type = Some(match result {
                        PropertyAccessResult::Success { type_id, .. } => type_id,
                        PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                            property_type.unwrap_or(TypeId::UNKNOWN)
                        }
                        PropertyAccessResult::IsUnknown => {
                            // TS2571: Object is of type 'unknown'
                            use crate::checker::types::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                access.expression,
                                "Object is of type 'unknown'.",
                                diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                            );
                            TypeId::ERROR
                        }
                        PropertyAccessResult::PropertyNotFound { .. } => {
                            report_no_index = true;
                            // Generate TS2339 for property not found during element access
                            self.error_property_not_exist_at(&property_name.to_string(), object_type_for_access, access.name_or_argument);
                            TypeId::ERROR  // Return ERROR instead of ANY to expose the error
                        }
                    });
                }
            }
        }

        let mut result_type = result_type.unwrap_or_else(|| {
            self.get_element_access_type(object_type_for_access, index_type, literal_index)
        });

        if use_index_signature_check
            && self.should_report_no_index_signature(
                object_type_for_access,
                index_type,
                literal_index,
            )
        {
            report_no_index = true;
        }

        if report_no_index {
            self.error_no_index_signature_at(index_type, object_type, access.name_or_argument);
        }

        if let Some(cause) = nullish_cause {
            if access.question_dot_token {
                result_type = self.ctx.types.union(vec![result_type, TypeId::UNDEFINED]);
            } else if !report_no_index {
                self.report_possibly_nullish_object(access.expression, cause);
            }
        }

        self.apply_flow_narrowing(idx, result_type)
    }

    fn get_element_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        use crate::solver::{LiteralValue, QueryDatabase, TypeKey};

        let object_key = match self.ctx.types.lookup(object_type) {
            Some(TypeKey::ReadonlyType(inner)) => self.ctx.types.lookup(inner),
            other => other,
        };

        let literal_index_type = literal_index
            .map(|index| self.ctx.types.literal_number(index as f64))
            .or_else(|| match self.ctx.types.lookup(index_type) {
                Some(TypeKey::Literal(LiteralValue::Number(num))) => {
                    Some(self.ctx.types.literal_number(num.0))
                }
                _ => None,
            });

        match object_key {
            Some(TypeKey::Array(element)) => {
                if let Some(literal_index_type) = literal_index_type {
                    let result = self
                        .ctx
                        .types
                        .evaluate_index_access(object_type, literal_index_type);
                    return if result == TypeId::UNDEFINED {
                        element
                    } else {
                        result
                    };
                }
                element
            }
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.ctx.types.tuple_list(elements);
                if let Some(literal_index_type) = literal_index_type {
                    let result = self
                        .ctx
                        .types
                        .evaluate_index_access(object_type, literal_index_type);
                    return if result == TypeId::UNDEFINED {
                        TypeId::ANY
                    } else {
                        result
                    };
                }

                let mut element_types: Vec<TypeId> =
                    elements.iter().map(|element| element.type_id).collect();
                if element_types.is_empty() {
                    TypeId::NEVER
                } else if element_types.len() == 1 {
                    element_types[0]
                } else {
                    self.ctx.types.union(element_types)
                }
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if literal_index.is_some() {
                    if let Some(number_index) = shape.number_index.as_ref() {
                        return number_index.value_type;
                    }
                    if let Some(string_index) = shape.string_index.as_ref() {
                        return string_index.value_type;
                    }
                    return TypeId::ERROR; // No matching index signature - expose error
                }

                if index_type == TypeId::NUMBER {
                    if let Some(number_index) = shape.number_index.as_ref() {
                        return number_index.value_type;
                    }
                    if let Some(string_index) = shape.string_index.as_ref() {
                        return string_index.value_type;
                    }
                    return TypeId::ERROR; // No matching index signature - expose error
                }

                if index_type == TypeId::STRING {
                    if let Some(string_index) = shape.string_index.as_ref() {
                        return string_index.value_type;
                    }
                }

                TypeId::ANY
            }
            Some(TypeKey::Union(members)) => {
                let members = self.ctx.types.type_list(members);
                let mut member_types = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    member_types.push(self.get_element_access_type(
                        member,
                        index_type,
                        literal_index,
                    ));
                }
                if member_types.is_empty() {
                    TypeId::ANY
                } else {
                    self.ctx.types.union(member_types)
                }
            }
            _ => {
                // For unresolved types, try to handle generic array types
                // Check if this might be a type parameter (like T in function<T>)
                // that should be treated as a generic element type

                // If we're accessing with a numeric index and can't resolve the object type,
                // it might be a generic array type parameter. In this case, return ANY
                // to allow the code to type-check properly.
                TypeId::ANY
            }
        }
    }

    fn split_nullish_type(&mut self, type_id: TypeId) -> (Option<TypeId>, Option<TypeId>) {
        use crate::solver::{IntrinsicKind, TypeKey};

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return (Some(type_id), None);
        };

        match key {
            TypeKey::Intrinsic(IntrinsicKind::Null) => (None, Some(TypeId::NULL)),
            TypeKey::Intrinsic(IntrinsicKind::Undefined | IntrinsicKind::Void) => {
                (None, Some(TypeId::UNDEFINED))
            }
            TypeKey::Union(members) => {
                let members = self.ctx.types.type_list(members);
                let mut non_null = Vec::with_capacity(members.len());
                let mut nullish = Vec::new();

                for &member in members.iter() {
                    match self.ctx.types.lookup(member) {
                        Some(TypeKey::Intrinsic(IntrinsicKind::Null)) => nullish.push(TypeId::NULL),
                        Some(TypeKey::Intrinsic(
                            IntrinsicKind::Undefined | IntrinsicKind::Void,
                        )) => {
                            nullish.push(TypeId::UNDEFINED);
                        }
                        _ => non_null.push(member),
                    }
                }

                if nullish.is_empty() {
                    return (Some(type_id), None);
                }

                let non_null_type = if non_null.is_empty() {
                    None
                } else if non_null.len() == 1 {
                    Some(non_null[0])
                } else {
                    Some(self.ctx.types.union(non_null))
                };

                let cause = if nullish.len() == 1 {
                    Some(nullish[0])
                } else {
                    Some(self.ctx.types.union(nullish))
                };

                (non_null_type, cause)
            }
            _ => (Some(type_id), None),
        }
    }

    fn report_possibly_nullish_object(&mut self, idx: NodeIndex, cause: TypeId) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let (code, message) = if cause == TypeId::NULL {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                "Object is possibly 'null'.",
            )
        } else if cause == TypeId::UNDEFINED {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                "Object is possibly 'undefined'.",
            )
        } else {
            (
                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                "Object is possibly 'null' or 'undefined'.",
            )
        };

        self.error_at_node(idx, message, code);
    }

    fn get_literal_index_from_node(&self, idx: NodeIndex) -> Option<usize> {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                return self.get_literal_index_from_node(paren.expression);
            }
        }

        if node.kind == SyntaxKind::NumericLiteral as u16 {
            if let Some(lit) = self.ctx.arena.get_literal(node) {
                if let Some(value) = lit.value {
                    if value.is_finite() && value.fract() == 0.0 && value >= 0.0 {
                        return Some(value as usize);
                    }
                }
            }
        }

        None
    }

    fn get_literal_string_from_node(&self, idx: NodeIndex) -> Option<String> {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                return self.get_literal_string_from_node(paren.expression);
            }
        }

        if let Some(symbol_name) = self.get_symbol_property_name_from_expr(idx) {
            return Some(symbol_name);
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.ctx.arena.get_literal(node).map(|lit| lit.text.clone());
        }

        None
    }

    fn merge_index_signature(
        target: &mut Option<crate::solver::IndexSignature>,
        incoming: crate::solver::IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type || existing.readonly != incoming.readonly
            {
                existing.value_type = TypeId::ERROR;
                existing.readonly = false;
            }
        } else {
            *target = Some(incoming);
        }
    }

    fn get_numeric_index_from_string(&self, value: &str) -> Option<usize> {
        let parsed: f64 = value.parse().ok()?;
        if !parsed.is_finite() || parsed.fract() != 0.0 || parsed < 0.0 {
            return None;
        }
        if parsed > (usize::MAX as f64) {
            return None;
        }
        Some(parsed as usize)
    }

    fn get_numeric_index_from_number(&self, value: f64) -> Option<usize> {
        if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
            return None;
        }
        if value > (usize::MAX as f64) {
            return None;
        }
        Some(value as usize)
    }

    fn get_literal_key_union_from_type(&self, index_type: TypeId) -> Option<(Vec<Atom>, Vec<f64>)> {
        use crate::solver::{LiteralValue, TypeKey};

        match self.ctx.types.lookup(index_type)? {
            TypeKey::Literal(LiteralValue::String(atom)) => Some((vec![atom], Vec::new())),
            TypeKey::Literal(LiteralValue::Number(num)) => Some((Vec::new(), vec![num.0])),
            TypeKey::Union(members) => {
                let members = self.ctx.types.type_list(members);
                let mut string_keys = Vec::with_capacity(members.len());
                let mut number_keys = Vec::new();
                for &member in members.iter() {
                    match self.ctx.types.lookup(member) {
                        Some(TypeKey::Literal(LiteralValue::String(atom))) => {
                            string_keys.push(atom)
                        }
                        Some(TypeKey::Literal(LiteralValue::Number(num))) => {
                            number_keys.push(num.0)
                        }
                        _ => return None,
                    }
                }
                Some((string_keys, number_keys))
            }
            _ => None,
        }
    }

    fn get_element_access_type_for_literal_keys(
        &mut self,
        object_type: TypeId,
        keys: &[Atom],
    ) -> Option<TypeId> {
        use crate::solver::{PropertyAccessResult, QueryDatabase};

        if keys.is_empty() {
            return None;
        }

        let numeric_as_index = self.is_array_like_type(object_type);
        let mut types = Vec::with_capacity(keys.len());

        for &key in keys {
            let name = self.ctx.types.resolve_atom(key);
            if numeric_as_index {
                if let Some(index) = self.get_numeric_index_from_string(&name) {
                    let element_type =
                        self.get_element_access_type(object_type, TypeId::NUMBER, Some(index));
                    types.push(element_type);
                    continue;
                }
            }

            match self.ctx.types.property_access_type(object_type, &name) {
                PropertyAccessResult::Success { type_id, .. } => types.push(type_id),
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    types.push(property_type.unwrap_or(TypeId::UNKNOWN));
                }
                // IsUnknown: Return None to signal that property access on unknown failed
                // The caller has node context and will report TS2571 error
                PropertyAccessResult::IsUnknown => return None,
                PropertyAccessResult::PropertyNotFound { .. } => return None,
            }
        }

        if types.len() == 1 {
            Some(types[0])
        } else {
            Some(self.ctx.types.union(types))
        }
    }

    fn get_element_access_type_for_literal_number_keys(
        &mut self,
        object_type: TypeId,
        keys: &[f64],
    ) -> Option<TypeId> {
        if keys.is_empty() {
            return None;
        }

        let mut types = Vec::with_capacity(keys.len());
        for &value in keys {
            if let Some(index) = self.get_numeric_index_from_number(value) {
                types.push(self.get_element_access_type(object_type, TypeId::NUMBER, Some(index)));
            } else {
                return Some(self.get_element_access_type(object_type, TypeId::NUMBER, None));
            }
        }

        if types.len() == 1 {
            Some(types[0])
        } else {
            Some(self.ctx.types.union(types))
        }
    }

    fn is_array_like_type(&self, object_type: TypeId) -> bool {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(object_type) {
            Some(TypeKey::Array(_) | TypeKey::Tuple(_)) => true,
            Some(TypeKey::ReadonlyType(inner)) => self.is_array_like_type(inner),
            Some(TypeKey::Union(members)) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .all(|member| self.is_array_like_type(*member))
            }
            Some(TypeKey::Intersection(members)) => {
                let members = self.ctx.types.type_list(members);
                members
                    .iter()
                    .any(|member| self.is_array_like_type(*member))
            }
            _ => false,
        }
    }

    fn should_report_no_index_signature(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        use crate::solver::TypeKey;

        if object_type == TypeId::ANY
            || object_type == TypeId::UNKNOWN
            || object_type == TypeId::ERROR
        {
            return false;
        }

        if index_type == TypeId::ANY || index_type == TypeId::UNKNOWN {
            return false;
        }

        let index_key_kind = self.get_index_key_kind(index_type);
        let wants_number = literal_index.is_some()
            || index_key_kind
                .as_ref()
                .is_some_and(|(_, wants_number)| *wants_number);
        let wants_string = index_key_kind
            .as_ref()
            .is_some_and(|(wants_string, _)| *wants_string);
        if !wants_number && !wants_string {
            return false;
        }

        let object_key = match self.ctx.types.lookup(object_type) {
            Some(TypeKey::ReadonlyType(inner)) => self.ctx.types.lookup(inner),
            other => other,
        };

        !self.is_element_indexable_key(&object_key, wants_string, wants_number)
    }

    fn get_index_key_kind(&self, index_type: TypeId) -> Option<(bool, bool)> {
        use crate::solver::{IntrinsicKind, LiteralValue, TypeKey};

        match self.ctx.types.lookup(index_type)? {
            TypeKey::Intrinsic(IntrinsicKind::String) => Some((true, false)),
            TypeKey::Intrinsic(IntrinsicKind::Number) => Some((false, true)),
            TypeKey::Literal(LiteralValue::String(_)) => Some((true, false)),
            TypeKey::Literal(LiteralValue::Number(_)) => Some((false, true)),
            TypeKey::Union(members) => {
                let members = self.ctx.types.type_list(members);
                let mut wants_string = false;
                let mut wants_number = false;
                for &member in members.iter() {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            _ => None,
        }
    }

    fn is_element_indexable_key(
        &self,
        object_key: &Option<crate::solver::TypeKey>,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        use crate::solver::{IntrinsicKind, LiteralValue, TypeKey};

        match object_key {
            Some(TypeKey::Array(_)) | Some(TypeKey::Tuple(_)) => wants_number,
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(*shape_id);
                let has_string = shape.string_index.is_some();
                let has_number = shape.number_index.is_some();
                (wants_string && has_string) || (wants_number && (has_number || has_string))
            }
            Some(TypeKey::Union(members)) => {
                let members = self.ctx.types.type_list(*members);
                members.iter().all(|member| {
                    let key = self.ctx.types.lookup(*member);
                    self.is_element_indexable_key(&key, wants_string, wants_number)
                })
            }
            Some(TypeKey::Intersection(members)) => {
                let members = self.ctx.types.type_list(*members);
                members.iter().any(|member| {
                    let key = self.ctx.types.lookup(*member);
                    self.is_element_indexable_key(&key, wants_string, wants_number)
                })
            }
            Some(TypeKey::Literal(LiteralValue::String(_))) => wants_number,
            Some(TypeKey::Intrinsic(IntrinsicKind::String)) => wants_number,
            _ => false,
        }
    }

    fn error_no_index_signature_at(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::solver::TypeFormatter;

        let mut formatter = TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols);
        let index_str = formatter.format(index_type);
        let object_str = formatter.format(object_type);
        let message = format!(
            "Element implicitly has an 'any' type because expression of type '{}' can't be used to index type '{}'.",
            index_str, object_str
        );

        self.error_at_node(idx, &message, diagnostic_codes::NO_INDEX_SIGNATURE);
    }

    /// Get type of conditional expression (ternary: a ? b : c).
    fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR; // Missing conditional expression data - propagate error
        };

        let when_true = self.get_type_of_node(cond.when_true);
        let when_false = self.get_type_of_node(cond.when_false);

        if when_true == when_false {
            when_true
        } else {
            // Use TypeInterner's union method for automatic normalization
            self.ctx.types.union(vec![when_true, when_false])
        }
    }

    /// Get type of function declaration/expression/arrow.
    fn get_type_of_function(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{FunctionShape, ParamInfo};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let (type_parameters, parameters, type_annotation, body, name_node, name_for_error) =
            if let Some(func) = self.ctx.arena.get_function(node) {
                let name_node = if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                };
                let name_for_error = if func.name.is_none() {
                    None
                } else {
                    self.get_function_name_from_node(idx)
                };
                (
                    &func.type_parameters,
                    &func.parameters,
                    func.type_annotation,
                    func.body,
                    name_node,
                    name_for_error,
                )
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                (
                    &method.type_parameters,
                    &method.parameters,
                    method.type_annotation,
                    method.body,
                    Some(method.name),
                    self.property_name_for_error(method.name),
                )
            } else {
                return TypeId::ERROR; // Missing function/method data - propagate error
            };

        // Function declarations don't report implicit any for parameters (handled by check_statement)
        let is_function_declaration = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        let is_method_or_constructor = matches!(
            node.kind,
            syntax_kind_ext::METHOD_DECLARATION | syntax_kind_ext::CONSTRUCTOR
        );
        let is_arrow_function = node.kind == syntax_kind_ext::ARROW_FUNCTION;

        // Check for duplicate parameter names in function expressions and arrow functions (TS2300)
        // Note: Methods and constructors are checked in check_method_declaration and check_constructor_declaration
        // Function declarations are checked in check_statement
        if !is_function_declaration && !is_method_or_constructor {
            self.check_duplicate_parameters(parameters);
        }

        let (type_params, type_param_updates) = self.push_type_parameters(type_parameters);

        // Collect parameter info using solver's ParamInfo struct
        let mut params = Vec::new();
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        // Setup contextual typing context if available
        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                ctx_type,
            ))
        } else {
            None
        };

        // For arrow functions, capture the outer `this` type to preserve lexical `this`
        // Arrow functions should inherit `this` from their enclosing scope
        let outer_this_type = if is_arrow_function {
            self.current_this_type()
        } else {
            None
        };

        let mut contextual_index = 0;
        for &param_idx in parameters.nodes.iter() {
            if let Some(param_node) = self.ctx.arena.get(param_idx) {
                if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                    // Get parameter name
                    let name = if let Some(name_node) = self.ctx.arena.get(param.name) {
                        if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                            Some(self.ctx.types.intern_string(&name_data.escaped_text))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let is_this_param = name == Some(this_atom);

                    let contextual_type = if let Some(ref helper) = ctx_helper {
                        helper.get_parameter_type(contextual_index)
                    } else {
                        None
                    };
                    // TS7006: Only count as contextual type if it's not UNKNOWN
                    // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                    let has_contextual_type = contextual_type.is_some_and(|t| t != TypeId::UNKNOWN);

                    // Use type annotation if present, otherwise infer from context
                    let type_id = if !param.type_annotation.is_none() {
                        // Check parameter type for parameter properties in function types
                        self.check_type_for_parameter_properties(param.type_annotation);
                        self.get_type_from_type_node(param.type_annotation)
                    } else if is_this_param {
                        // For `this` parameter without type annotation:
                        // - Arrow functions: inherit outer `this` type to preserve lexical scoping
                        // - Regular functions: use ANY (will trigger TS2683 when used, not TS2571)
                        // - Contextual type: if provided, use it (for function types with explicit `this`)
                        if let Some(ref helper) = ctx_helper {
                            helper.get_this_type().or(outer_this_type).unwrap_or(TypeId::ANY)
                        } else {
                            outer_this_type.unwrap_or(TypeId::ANY)
                        }
                    } else {
                        // Infer from contextual type
                        contextual_type.unwrap_or(TypeId::UNKNOWN)
                    };

                    if is_this_param {
                        if this_type.is_none() {
                            this_type = Some(type_id);
                        }
                        param_types.push(None);
                        continue;
                    }

                    if !is_function_declaration {
                        self.maybe_report_implicit_any_parameter(param, has_contextual_type);
                    }

                    // Check if optional or has initializer
                    let optional = param.question_token || !param.initializer.is_none();
                    let rest = param.dot_dot_dot_token;

                    params.push(ParamInfo {
                        name,
                        type_id,
                        optional,
                        rest,
                    });
                    param_types.push(Some(type_id));
                    contextual_index += 1;
                }
            }
        }

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in regular functions
        self.check_parameter_properties(&parameters.nodes);

        // Get return type from annotation or infer
        let has_type_annotation = !type_annotation.is_none();
        let (mut return_type, type_predicate) = if has_type_annotation {
            // Check return type for parameter properties in function types
            self.check_type_for_parameter_properties(type_annotation);
            self.return_type_and_predicate(type_annotation)
        } else {
            // Use UNKNOWN as default to enforce strict checking
            // This ensures return statements are checked even without annotation
            (TypeId::UNKNOWN, None)
        };

        // Evaluate Application types in return type to get their structural form
        // This allows proper comparison of return expressions against type alias applications like Reducer<S, A>
        return_type = self.evaluate_application_type(return_type);

        // Check the function body (for type errors within the body)
        if !body.is_none() {
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));

            // Check that parameter default values are assignable to declared types (TS2322)
            self.check_parameter_initializers(&parameters.nodes);

            let mut has_contextual_return = false;
            if !has_type_annotation {
                let return_context = ctx_helper
                    .as_ref()
                    .and_then(|helper| helper.get_return_type());
                // TS7010/TS7011: Only count as contextual return if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                has_contextual_return = return_context.is_some_and(|t| t != TypeId::UNKNOWN);
                return_type = self.infer_return_type_from_body(body, return_context);
            }

            // TS7010/TS7011 (implicit any return) is emitted for functions without
            // return type annotations when noImplicitAny is enabled and the return
            // type cannot be inferred (e.g., is 'any' or only returns undefined)
            // maybe_report_implicit_any_return handles the noImplicitAny check internally
            if !is_function_declaration {
                self.maybe_report_implicit_any_return(
                    name_for_error,
                    name_node,
                    return_type,
                    has_type_annotation,
                    has_contextual_return,
                    idx,
                );
            }

            // TS2705: Async function must return Promise
            // Check for arrow functions and function expressions
            // Note: Async generators (async function* or async *method) should NOT trigger TS2705
            // because they return AsyncGenerator or AsyncIterator, not Promise
            if !is_function_declaration && has_type_annotation {
                let (is_async, is_generator) = if let Some(func) = self.ctx.arena.get_function(node) {
                    (func.is_async, func.asterisk_token)
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    (self.has_async_modifier(&method.modifiers), method.asterisk_token)
                } else {
                    (false, false)
                };

                // Only check non-generator async functions
                if is_async && !is_generator && !self.is_promise_type(return_type) {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        type_annotation,
                        diagnostic_messages::ASYNC_FUNCTION_RETURNS_PROMISE,
                        diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                    );
                }

                // TS2705: Async function requires Promise constructor when Promise is not in lib
                if is_async && !is_generator && !self.ctx.has_promise_in_lib() {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    self.error_at_node(
                        idx,
                        diagnostic_messages::ASYNC_FUNCTION_REQUIRES_PROMISE_CONSTRUCTOR,
                        diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                    );
                }
            }

            // TS2366 (not all code paths return value) for function expressions and arrow functions
            // Check if all code paths return a value when return type requires it
            if !is_function_declaration && !body.is_none() {
                let check_return_type = return_type;
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(body);
                let falls_through = self.function_body_falls_through(body);

                // Determine if this is an async function
                let is_async = if let Some(func) = self.ctx.arena.get_function(node) {
                    func.is_async
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_async_modifier(&method.modifiers)
                } else {
                    false
                };

                // TS2355: Skip for async functions - they implicitly return Promise<void>
                // Async functions without a return statement automatically resolve to Promise<void>
                // so they should not emit "function must return a value" errors
                if has_type_annotation && requires_return && falls_through && !is_async {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    if !has_return {
                        self.error_at_node(
                            type_annotation,
                            "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                            diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                        );
                    } else {
                        self.error_at_node(
                            type_annotation,
                            diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                            diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                        );
                    }
                } else if self.ctx.no_implicit_returns && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    let error_node = if let Some(nn) = name_node { nn } else { body };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                    );
                }
            }

            // Determine if this is an async function for context tracking
            let is_async_for_context = if let Some(func) = self.ctx.arena.get_function(node) {
                func.is_async
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                self.has_async_modifier(&method.modifiers)
            } else {
                false
            };

            // Enter async context if applicable
            if is_async_for_context {
                self.ctx.enter_async_context();
            }

            // Push this_type to the stack before checking the body
            // This ensures this references inside the function have the proper type context
            // For functions with explicit this parameter: use that type
            // For arrow functions: use outer this type (already captured in this_type)
            // For regular functions without explicit this: this_type is None, which triggers TS2683 when this is used
            let mut pushed_this_type = false;
            if let Some(this_type) = this_type {
                self.ctx.this_type_stack.push(this_type);
                pushed_this_type = true;
            }

            self.push_return_type(return_type);
            self.check_statement(body);
            self.pop_return_type();

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }

            // Exit async context
            if is_async_for_context {
                self.ctx.exit_async_context();
            }
        }

        // Create function type using TypeInterner
        let shape = FunctionShape {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        };

        self.pop_type_parameters(type_param_updates);

        self.ctx.types.function(shape)
    }

    /// Get type of array literal.
    fn get_type_of_array_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::{TupleElement, TypeKey};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing array literal data - propagate error
        };

        if array.elements.nodes.is_empty() {
            // Empty array literal: infer from context or use never[]
            if let Some(contextual) = self.ctx.contextual_type {
                return contextual;
            }
            return self.ctx.types.array(TypeId::NEVER);
        }

        let tuple_context = match self.ctx.contextual_type {
            Some(ctx_type) => match self.ctx.types.lookup(ctx_type) {
                Some(TypeKey::Tuple(elements)) => {
                    let elements = self.ctx.types.tuple_list(elements);
                    Some(elements.as_ref().to_vec())
                }
                _ => None,
            },
            None => None,
        };

        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            Some(ContextualTypeContext::with_expected(
                self.ctx.types,
                ctx_type,
            ))
        } else {
            None
        };

        // Get types of all elements, applying contextual typing when available.
        let mut element_types = Vec::new();
        let mut tuple_elements = Vec::new();
        for (index, &elem_idx) in array.elements.nodes.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }

            let prev_context = self.ctx.contextual_type;
            if let Some(ref helper) = ctx_helper {
                if tuple_context.is_some() {
                    self.ctx.contextual_type = helper.get_tuple_element_type(index);
                } else {
                    self.ctx.contextual_type = helper.get_array_element_type();
                }
            }

            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let elem_is_spread = elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT;
            let elem_type = if elem_is_spread {
                if let Some(spread_data) = self.ctx.arena.get_spread(elem_node) {
                    self.get_type_of_node(spread_data.expression)
                } else {
                    TypeId::ANY
                }
            } else {
                self.get_type_of_node(elem_idx)
            };

            self.ctx.contextual_type = prev_context;

            if let Some(ref expected) = tuple_context {
                let (name, optional) = match expected.get(index) {
                    Some(el) => (el.name, el.optional),
                    None => (None, false),
                };
                tuple_elements.push(TupleElement {
                    type_id: elem_type,
                    name,
                    optional,
                    rest: elem_is_spread,
                });
            } else {
                element_types.push(elem_type);
            }
        }

        if tuple_context.is_some() {
            return self.ctx.types.tuple(tuple_elements);
        }

        if let Some(ref helper) = ctx_helper {
            if let Some(context_element_type) = helper.get_array_element_type() {
                if element_types
                    .iter()
                    .all(|&elem_type| self.is_assignable_to(elem_type, context_element_type))
                {
                    return self.ctx.types.array(context_element_type);
                }
            }
        }

        // Choose a best common type if any element is a supertype of all others.
        let element_type = if element_types.len() == 1 {
            element_types[0]
        } else if element_types.is_empty() {
            TypeId::NEVER
        } else {
            let mut best = None;
            'candidates: for &candidate in &element_types {
                for &elem in &element_types {
                    if !self.is_assignable_to(elem, candidate) {
                        continue 'candidates;
                    }
                }
                best = Some(candidate);
                break;
            }
            best.unwrap_or_else(|| self.ctx.types.union(element_types))
        };

        self.ctx.types.array(element_type)
    }

    /// Get type of object literal.
    fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };
        use crate::solver::{PropertyInfo, QueryDatabase};
        use rustc_hash::FxHashMap;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return TypeId::ERROR; // Missing object literal data - propagate error
        };

        // Collect properties from the object literal (later entries override earlier ones)
        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                if let Some(name) = self.get_property_name(prop.name) {
                    // Set contextual type for property value
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    let value_type = self.get_type_of_node(prop.initializer);

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the property value type is 'any'
                    if self.ctx.no_implicit_any && prev_context.is_some() && value_type == TypeId::ANY {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICIT_ANY,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_MEMBER,
                        );
                    }

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            prop.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Shorthand property: { x } - identifier is both name and value
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(ident) = self.ctx.arena.get_identifier(elem_node) {
                    let name = ident.escaped_text.clone();

                    // Set contextual type for shorthand property value
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    let value_type = self.get_type_of_node(elem_idx);

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    // TS7008: Member implicitly has an 'any' type
                    // Report this error when noImplicitAny is enabled, the object literal has a contextual type,
                    // and the shorthand property value type is 'any'
                    if self.ctx.no_implicit_any
                        && prev_context.is_some()
                        && value_type == TypeId::ANY
                    {
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICIT_ANY,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_MEMBER,
                        );
                    }

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            elem_idx,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: value_type,
                            write_type: value_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Method shorthand: { foo() {} }
            else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                if let Some(name) = self.get_property_name(method.name) {
                    // Set contextual type for method
                    let prev_context = self.ctx.contextual_type;
                    if let Some(ctx_type) = prev_context {
                        self.ctx.contextual_type =
                            self.ctx.types.contextual_property_type(ctx_type, &name);
                    }

                    let method_type = self.get_type_of_function(elem_idx);

                    // Restore context
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            method.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Accessor: { get foo() {} } or { set foo(v) {} }
            else if let Some(accessor) = self.ctx.arena.get_accessor(elem_node) {
                // Check for missing body - error 1005 at end of accessor
                if accessor.body.is_none() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    // Report at accessor.end - 1 (pointing to the closing paren)
                    let end_pos = elem_node.end.saturating_sub(1);
                    self.error_at_position(
                        end_pos,
                        1,
                        "'{' expected.",
                        diagnostic_codes::TOKEN_EXPECTED,
                    );
                }

                // For setters, check implicit any on parameters (error 7006)
                if elem_node.kind == syntax_kind_ext::SET_ACCESSOR {
                    for &param_idx in &accessor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                self.maybe_report_implicit_any_parameter(param, false);
                            }
                        }
                    }
                }

                if let Some(name) = self.get_property_name(accessor.name) {
                    // For getter, infer return type; for setter, it's void
                    let accessor_type = if elem_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        self.get_type_of_function(elem_idx)
                    } else {
                        TypeId::VOID
                    };
                    let name_atom = self.ctx.types.intern_string(&name);

                    // Check for duplicate property
                    if properties.contains_key(&name_atom) {
                        let message = format_message(
                            diagnostic_messages::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                            &[&name],
                        );
                        self.error_at_node(
                            accessor.name,
                            &message,
                            diagnostic_codes::OBJECT_LITERAL_DUPLICATE_PROPERTY,
                        );
                    }

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id: accessor_type,
                            write_type: accessor_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                        },
                    );
                }
            }
            // Spread assignment: { ...obj }
            else if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                let spread_expr = self
                    .ctx
                    .arena
                    .get_spread(elem_node)
                    .map(|spread| spread.expression)
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_unary_expr_ex(elem_node)
                            .map(|unary| unary.expression)
                    });
                if let Some(spread_expr) = spread_expr {
                    let spread_type = self.get_type_of_node(spread_expr);
                    for prop in self.collect_object_spread_properties(spread_type) {
                        properties.insert(prop.name, prop);
                    }
                }
            }
            // Skip computed properties for now
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        self.ctx.types.object(properties)
    }

    fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::solver::TypeKey;
        use rustc_hash::FxHashMap;

        let resolved = self.resolve_type_for_property_access(type_id);
        match self.ctx.types.lookup(resolved) {
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.iter().cloned().collect()
            }
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.properties.iter().cloned().collect()
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                let mut merged: FxHashMap<Atom, crate::solver::PropertyInfo> = FxHashMap::default();
                for &member in members.iter() {
                    for prop in self.collect_object_spread_properties(member) {
                        merged.insert(prop.name, prop);
                    }
                }
                merged.into_values().collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get property name as string from a property name node (identifier, string literal, etc.)
    fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        use crate::scanner::SyntaxKind;

        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                if !lit.text.is_empty() {
                    return Some(lit.text.clone());
                }
            }
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.ctx.arena.get_computed_property(name_node) {
                if let Some(symbol_name) =
                    self.get_symbol_property_name_from_expr(computed.expression)
                {
                    return Some(symbol_name);
                }
                if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
                    if matches!(
                        expr_node.kind,
                        k if k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                    ) {
                        if let Some(lit) = self.ctx.arena.get_literal(expr_node) {
                            if !lit.text.is_empty() {
                                return Some(lit.text.clone());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn get_symbol_property_name_from_expr(&self, expr_idx: NodeIndex) -> Option<String> {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.get_symbol_property_name_from_expr(paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("Symbol.{}", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) {
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                if !lit.text.is_empty() {
                    return Some(format!("Symbol.{}", lit.text));
                }
            }
        }

        None
    }

    /// Get type of prefix unary expression.
    fn get_type_of_prefix_unary(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return TypeId::ERROR; // Missing unary expression data - propagate error
        };

        match unary.operator {
            // ! returns boolean
            k if k == SyntaxKind::ExclamationToken as u16 => TypeId::BOOLEAN,
            // typeof returns string but still type-check operand for flow/node types.
            k if k == SyntaxKind::TypeOfKeyword as u16 => {
                self.get_type_of_node(unary.operand);
                TypeId::STRING
            }
            // Unary + and - return number unless contextual typing expects a numeric literal.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                if let Some(literal_type) = self.literal_type_from_initializer(idx) {
                    if self.contextual_literal_type(literal_type).is_some() {
                        return literal_type;
                    }
                }
                TypeId::NUMBER
            }
            // ~ returns number
            k if k == SyntaxKind::TildeToken as u16 => TypeId::NUMBER,
            // ++ and -- return number
            k if k == SyntaxKind::PlusPlusToken as u16
                || k == SyntaxKind::MinusMinusToken as u16 =>
            {
                TypeId::NUMBER
            }
            _ => TypeId::ANY,
        }
    }

    /// Get type of template expression (template literal with substitutions).
    /// Type-checks all expressions within template spans to emit errors like TS2304.
    fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Type-check each template span's expression
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            self.get_type_of_node(span.expression);
        }

        // Template expressions always produce string type
        TypeId::STRING
    }

    // =========================================================================
    // Type Relations (uses solver::CompatChecker for assignability)
    // =========================================================================

    fn enum_symbol_from_type(&self, type_id: TypeId) -> Option<SymbolId> {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(type_id) else {
            return None;
        };
        let symbol = self.ctx.binder.get_symbol(SymbolId(sym_id))?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(SymbolId(sym_id))
    }

    fn enum_symbol_from_value_type(&self, type_id: TypeId) -> Option<SymbolId> {
        use crate::solver::{SymbolRef, TypeKey};

        let sym_id = match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Ref(SymbolRef(sym_id))) => sym_id,
            Some(TypeKey::TypeQuery(SymbolRef(sym_id))) => sym_id,
            _ => return None,
        };

        let symbol = self.ctx.binder.get_symbol(SymbolId(sym_id))?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(SymbolId(sym_id))
    }

    fn enum_object_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        use crate::solver::{IndexSignature, ObjectShape, PropertyInfo};
        use rustc_hash::FxHashMap;

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let member_type = match self.enum_kind(sym_id) {
            Some(EnumKind::String) => TypeId::STRING,
            Some(EnumKind::Numeric) => TypeId::NUMBER,
            None => {
                // Return UNKNOWN instead of ANY for enum without explicit kind
                TypeId::UNKNOWN
            }
        };

        let mut props: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(member.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);
                props.entry(name_atom).or_insert(PropertyInfo {
                    name: name_atom,
                    type_id: member_type,
                    write_type: member_type,
                    optional: false,
                    readonly: true,
                    is_method: false,
                });
            }
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        if self.enum_kind(sym_id) == Some(EnumKind::Numeric) {
            let number_index = Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: true,
            });
            return Some(self.ctx.types.object_with_index(ObjectShape {
                properties,
                string_index: None,
                number_index,
            }));
        }

        Some(self.ctx.types.object(properties))
    }

    fn enum_kind(&self, sym_id: SymbolId) -> Option<EnumKind> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.ctx.arena.get(decl_idx)?;
        let enum_decl = self.ctx.arena.get_enum(node)?;

        let mut saw_string = false;
        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };
            if member.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                continue;
            };
            if init_node.kind == SyntaxKind::StringLiteral as u16
                || init_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                saw_string = true;
                break;
            }
        }

        if saw_string {
            Some(EnumKind::String)
        } else {
            Some(EnumKind::Numeric)
        }
    }

    /// Get the type of an enum member (STRING or NUMBER) by finding its parent enum.
    /// This is used when enum members are accessed through namespace exports.
    fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        use crate::parser::node_flags;

        // Get the extended node to find parent
        let Some(ext) = self.ctx.arena.get_extended(member_decl) else {
            return TypeId::ERROR; // Missing extended node data - propagate error
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return TypeId::ERROR; // Missing parent - propagate error
        }

        // Walk up to find the enum declaration
        let mut current = parent_idx;
        let max_depth = 10; // Prevent infinite loops
        for _ in 0..max_depth {
            let Some(parent_node) = self.ctx.arena.get(current) else {
                break;
            };

            // Found the enum declaration
            if parent_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                let Some(enum_decl) = self.ctx.arena.get_enum(parent_node) else {
                    break;
                };

                // Check if any member has a string initializer
                for &member_idx in &enum_decl.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                        continue;
                    };
                    if member.initializer.is_none() {
                        continue;
                    }
                    let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                        continue;
                    };
                    if init_node.kind == SyntaxKind::StringLiteral as u16
                        || init_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    {
                        return TypeId::STRING;
                    }
                }

                // No string initializer found, so it's a numeric enum
                return TypeId::NUMBER;
            }

            // Move to parent
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
        }

        TypeId::ANY
    }

    fn enum_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        let source_enum = self.enum_symbol_from_type(source);
        let target_enum = self.enum_symbol_from_type(target);

        if let (Some(source_enum), Some(target_enum)) = (source_enum, target_enum) {
            return Some(source_enum == target_enum);
        }

        if let Some(source_enum) = source_enum {
            if self.enum_kind(source_enum) == Some(EnumKind::Numeric) {
                if let Some(env) = env {
                    let mut checker =
                        crate::solver::CompatChecker::with_resolver(self.ctx.types, env);
                    checker.set_strict_null_checks(self.ctx.strict_null_checks);
                    return Some(checker.is_assignable(TypeId::NUMBER, target));
                }
                let mut checker = crate::solver::CompatChecker::new(self.ctx.types);
                checker.set_strict_null_checks(self.ctx.strict_null_checks);
                return Some(checker.is_assignable(TypeId::NUMBER, target));
            }
        }

        if let Some(target_enum) = target_enum {
            if self.enum_kind(target_enum) == Some(EnumKind::Numeric) {
                if let Some(env) = env {
                    let mut checker =
                        crate::solver::CompatChecker::with_resolver(self.ctx.types, env);
                    checker.set_strict_null_checks(self.ctx.strict_null_checks);
                    return Some(checker.is_assignable(source, TypeId::NUMBER));
                }
                let mut checker = crate::solver::CompatChecker::new(self.ctx.types);
                checker.set_strict_null_checks(self.ctx.strict_null_checks);
                return Some(checker.is_assignable(source, TypeId::NUMBER));
            }
        }

        None
    }

    fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        use crate::solver::TypeKey;

        // Check if source is an abstract constructor
        let source_is_abstract = self.is_abstract_constructor_type(source, env);

        // Additional check: if source_is_abstract is false, check symbol_types directly
        let source_is_abstract_from_symbols = if !source_is_abstract {
            let mut found_abstract = false;
            for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                if cached_type == source {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        if symbol.flags & symbol_flags::CLASS != 0
                            && symbol.flags & symbol_flags::ABSTRACT != 0
                        {
                            found_abstract = true;
                            break;
                        }
                    }
                }
            }
            found_abstract
        } else {
            false
        };

        let final_source_is_abstract = source_is_abstract || source_is_abstract_from_symbols;

        if !final_source_is_abstract {
            return None;
        }

        // Source is abstract - check if target is a non-abstract constructor type
        let target_is_abstract = self.is_abstract_constructor_type(target, env);

        // Also check target via symbol_types
        let target_is_abstract_from_symbols = {
            let mut found_abstract = false;
            for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                if cached_type == target {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        if symbol.flags & symbol_flags::CLASS != 0
                            && symbol.flags & symbol_flags::ABSTRACT != 0
                        {
                            found_abstract = true;
                            break;
                        }
                    }
                }
            }
            found_abstract
        };

        let final_target_is_abstract = target_is_abstract || target_is_abstract_from_symbols;

        // If target is also abstract, allow the assignment (abstract to abstract is OK)
        if final_target_is_abstract {
            return None;
        }

        // Check if target is a constructor type (has construct signatures)
        let target_is_constructor = match self.ctx.types.lookup(target) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                !shape.construct_signatures.is_empty()
            }
            _ => false,
        };

        // If target is a constructor type but not abstract, reject the assignment
        if target_is_constructor {
            return Some(false);
        }

        None
    }

    fn constructor_access_rank(level: Option<MemberAccessLevel>) -> u8 {
        match level {
            Some(MemberAccessLevel::Private) => 2,
            Some(MemberAccessLevel::Protected) => 1,
            None => 0,
        }
    }

    fn constructor_access_name(level: Option<MemberAccessLevel>) -> &'static str {
        match level {
            Some(MemberAccessLevel::Private) => "private",
            Some(MemberAccessLevel::Protected) => "protected",
            None => "public",
        }
    }

    fn constructor_access_level(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
        visited: &mut FxHashSet<TypeId>,
    ) -> Option<MemberAccessLevel> {
        use crate::solver::TypeKey;

        if !visited.insert(type_id) {
            return None;
        }

        if self.ctx.private_constructor_types.contains(&type_id) {
            return Some(MemberAccessLevel::Private);
        }
        if self.ctx.protected_constructor_types.contains(&type_id) {
            return Some(MemberAccessLevel::Protected);
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return None;
        };

        match key {
            TypeKey::Ref(symbol) | TypeKey::TypeQuery(symbol) => self
                .resolve_type_env_symbol(symbol, env)
                .and_then(|resolved| {
                    if resolved != type_id {
                        self.constructor_access_level(resolved, env, visited)
                    } else {
                        None
                    }
                }),
            TypeKey::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                if app.base != type_id {
                    self.constructor_access_level(app.base, env, visited)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn constructor_access_level_for_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<MemberAccessLevel> {
        let mut visited = FxHashSet::default();
        self.constructor_access_level(type_id, env, &mut visited)
    }

    fn constructor_accessibility_mismatch(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        let source_level = self.constructor_access_level_for_type(source, env);
        let target_level = self.constructor_access_level_for_type(target, env);

        if source_level.is_none() && target_level.is_none() {
            return None;
        }

        let source_rank = Self::constructor_access_rank(source_level);
        let target_rank = Self::constructor_access_rank(target_level);
        if source_rank > target_rank {
            return Some((source_level, target_level));
        }
        None
    }

    fn constructor_accessibility_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        if self
            .constructor_accessibility_mismatch(source, target, env)
            .is_some()
        {
            return Some(false);
        }
        None
    }

    /// Private brand assignability override.
    /// If both source and target types have private brands, they must match exactly.
    /// This implements nominal typing for classes with private fields.
    fn private_brand_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        _env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        let source_brand = self.get_private_brand(source);
        let target_brand = self.get_private_brand(target);

        match (source_brand, target_brand) {
            (Some(brand1), Some(brand2)) => {
                // Both types have private brands - they must match exactly
                Some(brand1 == brand2)
            }
            (Some(_), None) | (None, Some(_)) => {
                // One type has a private brand, the other doesn't
                // This is not assignable
                Some(false)
            }
            (None, None) => None, // Neither has private brand, fall through to normal check
        }
    }

    fn class_symbol_from_expression(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(expr_idx)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & symbol_flags::CLASS != 0 {
                return Some(sym_id);
            }
        }
        None
    }

    fn class_symbol_from_type_annotation(&self, type_idx: NodeIndex) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return None;
        };
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.ctx.arena.get_type_query(node)?;
        self.class_symbol_from_expression(query.expr_name)
    }

    fn assignment_target_class_symbol(&self, left_idx: NodeIndex) -> Option<SymbolId> {
        let Some(node) = self.ctx.arena.get(left_idx) else {
            return None;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(left_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            return Some(sym_id);
        }
        if symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            == 0
        {
            return None;
        }
        if symbol.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if !var_decl.type_annotation.is_none() {
            if let Some(class_sym) =
                self.class_symbol_from_type_annotation(var_decl.type_annotation)
            {
                return Some(class_sym);
            }
        }
        if !var_decl.initializer.is_none() {
            if let Some(class_sym) = self.class_symbol_from_expression(var_decl.initializer) {
                return Some(class_sym);
            }
        }
        None
    }

    fn class_constructor_access_level(&self, sym_id: SymbolId) -> Option<MemberAccessLevel> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;
        let mut access: Option<MemberAccessLevel> = None;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            if self.has_private_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Private);
            }
            if self.has_protected_modifier(&ctor.modifiers) {
                access = Some(MemberAccessLevel::Protected);
            }
        }
        access
    }

    fn constructor_accessibility_mismatch_for_assignment(
        &self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        let source_sym = self.class_symbol_from_expression(right_idx)?;
        let target_sym = self.assignment_target_class_symbol(left_idx)?;
        let source_level = self.class_constructor_access_level(source_sym);
        let target_level = self.class_constructor_access_level(target_sym);
        if source_level.is_none() && target_level.is_none() {
            return None;
        }
        if Self::constructor_access_rank(source_level) > Self::constructor_access_rank(target_level)
        {
            return Some((source_level, target_level));
        }
        None
    }

    fn constructor_accessibility_mismatch_for_var_decl(
        &self,
        var_decl: &crate::parser::thin_node::VariableDeclarationData,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        if var_decl.initializer.is_none() {
            return None;
        }
        let source_sym = self.class_symbol_from_expression(var_decl.initializer)?;
        let target_sym = self.class_symbol_from_type_annotation(var_decl.type_annotation)?;
        let source_level = self.class_constructor_access_level(source_sym);
        let target_level = self.class_constructor_access_level(target_sym);
        if source_level.is_none() && target_level.is_none() {
            return None;
        }
        if Self::constructor_access_rank(source_level) > Self::constructor_access_rank(target_level)
        {
            return Some((source_level, target_level));
        }
        None
    }

    fn resolve_type_env_symbol(
        &self,
        symbol: crate::solver::SymbolRef,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<TypeId> {
        if let Some(env) = env {
            return env.get(symbol);
        }
        let env_ref = self.ctx.type_env.borrow();
        env_ref.get(symbol)
    }

    fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        match key {
            TypeKey::TypeQuery(SymbolRef(sym_id)) => self.get_type_of_symbol(SymbolId(sym_id)),
            TypeKey::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                if let Some(TypeKey::TypeQuery(SymbolRef(sym_id))) = self.ctx.types.lookup(app.base)
                {
                    let base = self.get_type_of_symbol(SymbolId(sym_id));
                    return self.ctx.types.application(base, app.args.clone());
                }
                type_id
            }
            _ => type_id,
        }
    }

    fn find_enclosing_source_file(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::SOURCE_FILE {
                    return Some(current);
                }
            }
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        None
    }

    fn jsdoc_type_annotation_for_node(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let root = self.find_enclosing_source_file(idx)?;
        let source_text = self
            .ctx
            .arena
            .get(root)
            .and_then(|node| self.ctx.arena.get_source_file(node))
            .map(|sf| sf.text.as_str())?;
        let jsdoc = crate::lsp::jsdoc::jsdoc_for_node(self.ctx.arena, root, idx, source_text);
        if jsdoc.is_empty() {
            return None;
        }
        let type_text = self.extract_jsdoc_type(&jsdoc)?;
        self.parse_jsdoc_type(&type_text)
    }

    fn extract_jsdoc_type(&self, doc: &str) -> Option<String> {
        let tag_pos = doc.find("@type")?;
        let rest = &doc[tag_pos + "@type".len()..];
        let open = rest.find('{')?;
        let after_open = &rest[open + 1..];
        let close = after_open.find('}')?;
        let type_text = after_open[..close].trim();
        if type_text.is_empty() {
            None
        } else {
            Some(type_text.to_string())
        }
    }

    fn parse_jsdoc_type(&mut self, text: &str) -> Option<TypeId> {
        use crate::solver::FunctionShape;
        use crate::solver::ParamInfo;

        fn skip_ws(text: &str, pos: &mut usize) {
            while *pos < text.len() && text.as_bytes()[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
        }

        fn parse_ident<'a>(text: &'a str, pos: &mut usize) -> Option<&'a str> {
            let start = *pos;
            while *pos < text.len() {
                let ch = text.as_bytes()[*pos] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    *pos += 1;
                } else {
                    break;
                }
            }
            if *pos > start {
                Some(&text[start..*pos])
            } else {
                None
            }
        }

        fn parse_type(
            checker: &mut ThinCheckerState,
            text: &str,
            pos: &mut usize,
        ) -> Option<TypeId> {
            skip_ws(text, pos);
            if text[*pos..].starts_with("function") {
                return parse_function_type(checker, text, pos);
            }

            let ident = parse_ident(text, pos)?;
            let type_id = match ident {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "void" => TypeId::VOID,
                "any" => TypeId::ANY,
                "unknown" => TypeId::UNKNOWN,
                _ => TypeId::ANY,
            };
            Some(type_id)
        }

        fn parse_function_type(
            checker: &mut ThinCheckerState,
            text: &str,
            pos: &mut usize,
        ) -> Option<TypeId> {
            if !text[*pos..].starts_with("function") {
                return None;
            }
            *pos += "function".len();
            skip_ws(text, pos);
            if *pos >= text.len() || text.as_bytes()[*pos] != b'(' {
                return None;
            }
            *pos += 1;
            let mut params = Vec::new();
            loop {
                skip_ws(text, pos);
                if *pos >= text.len() {
                    return None;
                }
                if text.as_bytes()[*pos] == b')' {
                    *pos += 1;
                    break;
                }
                let param_type = parse_type(checker, text, pos)?;
                params.push(ParamInfo {
                    name: None,
                    type_id: param_type,
                    optional: false,
                    rest: false,
                });
                skip_ws(text, pos);
                if *pos < text.len() && text.as_bytes()[*pos] == b',' {
                    *pos += 1;
                    continue;
                }
                if *pos < text.len() && text.as_bytes()[*pos] == b')' {
                    *pos += 1;
                    break;
                }
            }
            skip_ws(text, pos);
            if *pos >= text.len() || text.as_bytes()[*pos] != b':' {
                return None;
            }
            *pos += 1;
            let return_type = parse_type(checker, text, pos)?;
            let shape = FunctionShape {
                type_params: Vec::new(),
                params,
                this_type: None,
                return_type,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            };
            Some(checker.ctx.types.function(shape))
        }

        let mut pos = 0;
        let type_id = parse_type(self, text, &mut pos)?;
        Some(type_id)
    }

    fn is_abstract_constructor_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> bool {
        use crate::binder::SymbolId;
        use crate::solver::TypeKey;

        // First check the cached set
        if self.ctx.abstract_constructor_types.contains(&type_id) {
            return true;
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::TypeQuery(symbol) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(symbol.0)) {
                    // Check if the symbol is marked as abstract
                    if symbol.flags & symbol_flags::ABSTRACT != 0 {
                        return true;
                    }
                    // Also check if this is an abstract class by examining its declaration
                    // The ABSTRACT flag might not be set on the symbol, so check the class modifiers
                    if symbol.flags & symbol_flags::CLASS != 0 {
                        // Get the class declaration and check if it has the abstract modifier
                        let decl_idx = if !symbol.value_declaration.is_none() {
                            symbol.value_declaration
                        } else {
                            symbol
                                .declarations
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE)
                        };
                        if !decl_idx.is_none() {
                            if let Some(node) = self.ctx.arena.get(decl_idx) {
                                if let Some(class) = self.ctx.arena.get_class(node) {
                                    return self.has_abstract_modifier(&class.modifiers);
                                }
                            }
                        }
                    }
                    false
                } else {
                    false
                }
            }
            TypeKey::Ref(symbol) => self
                .resolve_type_env_symbol(symbol, env)
                .map(|resolved| {
                    resolved != type_id && self.is_abstract_constructor_type(resolved, env)
                })
                .unwrap_or(false),
            TypeKey::Callable(shape_id) => {
                // For Callable types (constructor types), check if they're in the abstract set
                // This handles `typeof AbstractClass` which returns a Callable type
                if self.ctx.abstract_constructor_types.contains(&type_id) {
                    return true;
                }
                // Additional check: iterate through symbol_types to find matching class symbols
                // This handles cases where the type wasn't added to abstract_constructor_types
                // or the type is being compared before being cached
                for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                    if cached_type == type_id {
                        // Found a symbol with this type, check if it's an abstract class
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            if symbol.flags & symbol_flags::CLASS != 0
                                && symbol.flags & symbol_flags::ABSTRACT != 0
                            {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            TypeKey::Application(app_id) => {
                // For generic type applications, check the base type
                let app = self.ctx.types.type_application(app_id);
                self.is_abstract_constructor_type(app.base, env)
            }
            _ => false,
        }
    }

    fn is_concrete_constructor_target(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.is_concrete_constructor_target_inner(type_id, env, &mut visited)
    }

    fn is_concrete_constructor_target_inner(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        use crate::solver::TypeKey;

        if !visited.insert(type_id) {
            return false;
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::Function(func_id) => self.ctx.types.function_shape(func_id).is_constructor,
            TypeKey::Callable(shape_id) => {
                // A Callable is a concrete constructor target if it has construct signatures
                // AND it's not an abstract constructor
                let has_construct = !self
                    .ctx
                    .types
                    .callable_shape(shape_id)
                    .construct_signatures
                    .is_empty();
                let is_abstract = self.ctx.abstract_constructor_types.contains(&type_id);
                has_construct && !is_abstract
            }
            TypeKey::TypeQuery(symbol) | TypeKey::Ref(symbol) => {
                // First try to resolve via TypeEnvironment
                if let Some(resolved) = self.resolve_type_env_symbol(symbol, env) {
                    if resolved != type_id {
                        return self.is_concrete_constructor_target_inner(resolved, env, visited);
                    }
                }
                // Fallback: Check if the symbol is a non-abstract class or interface with construct signatures
                // This handles `typeof ConcreteClass` and `typeof InterfaceWithConstructSig` when TypeEnvironment lookup fails
                use crate::binder::SymbolId;
                use crate::solver::SymbolRef;
                if let Some(sym) = self.ctx.binder.get_symbol(SymbolId(symbol.0)) {
                    // A non-abstract class is a concrete constructor target
                    if sym.flags & symbol_flags::CLASS != 0
                        && sym.flags & symbol_flags::ABSTRACT == 0
                    {
                        return true;
                    }
                    // An interface with construct signatures is also a concrete constructor target
                    // (interfaces are never abstract)
                    if sym.flags & symbol_flags::INTERFACE != 0 {
                        // Check if the interface has a construct signature by examining its type
                        let decl_idx = sym.value_declaration;
                        if !decl_idx.is_none() {
                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                if let Some(interface_data) =
                                    self.ctx.arena.get_interface(decl_node)
                                {
                                    // Check members for construct signatures
                                    for &member_idx in &interface_data.members.nodes {
                                        if let Some(member_node) = self.ctx.arena.get(member_idx) {
                                            // CONSTRUCT_SIGNATURE = 194
                                            if member_node.kind == 194 {
                                                return true; // Interface has construct signature
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                false
            }
            TypeKey::Union(_) | TypeKey::Intersection(_) => false,
            _ => false,
        }
    }

    /// Evaluate specific type constructs that are not directly handled in assignability.
    fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        match key {
            TypeKey::Application(_) => self.evaluate_type_with_resolution(type_id),
            TypeKey::IndexAccess(_, _)
            | TypeKey::KeyOf(_)
            | TypeKey::Mapped(_)
            | TypeKey::Conditional(_) => self.evaluate_type_with_env(type_id),
            _ => type_id,
        }
    }

    /// Check if `source` type is assignable to `target` type.
    ///
    /// Uses the solver's SubtypeChecker with coinductive cycle detection.
    /// Uses the context's TypeEnvironment for resolving type references and expanding Applications.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::solver::CompatChecker;

        self.ensure_application_symbols_resolved(source);
        self.ensure_application_symbols_resolved(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let env = self.ctx.type_env.borrow();
        if let Some(result) =
            self.abstract_constructor_assignability_override(source, target, Some(&*env))
        {
            return result;
        }
        if let Some(result) = self.constructor_accessibility_override(source, target, Some(&*env)) {
            return result;
        }
        if let Some(result) = self.private_brand_assignability_override(source, target, Some(&*env)) {
            return result;
        }
        if let Some(result) = self.enum_assignability_override(source, target, Some(&*env)) {
            return result;
        }

        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        checker.set_strict_function_types(self.ctx.strict_function_types);
        checker.set_strict_null_checks(self.ctx.strict_null_checks);
        checker.is_assignable(source, target)
    }

    /// Check if `source` type is assignable to `target` type, resolving Ref types.
    ///
    /// Uses the provided TypeEnvironment to resolve type references.
    pub fn is_assignable_to_with_env(
        &self,
        source: TypeId,
        target: TypeId,
        env: &crate::solver::TypeEnvironment,
    ) -> bool {
        use crate::solver::CompatChecker;

        if let Some(result) =
            self.abstract_constructor_assignability_override(source, target, Some(env))
        {
            return result;
        }
        if let Some(result) = self.constructor_accessibility_override(source, target, Some(env)) {
            return result;
        }
        if let Some(result) = self.private_brand_assignability_override(source, target, Some(env)) {
            return result;
        }
        if let Some(result) = self.enum_assignability_override(source, target, Some(env)) {
            return result;
        }

        let mut checker = CompatChecker::with_resolver(self.ctx.types, env);
        checker.set_strict_function_types(self.ctx.strict_function_types);
        checker.set_strict_null_checks(self.ctx.strict_null_checks);
        checker.is_assignable(source, target)
    }

    fn should_skip_weak_union_error(
        &self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        use crate::solver::CompatChecker;

        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let env = self.ctx.type_env.borrow();
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        checker.set_strict_null_checks(self.ctx.strict_null_checks);
        checker.is_weak_union_violation(source, target)
    }

    /// Check if `source` type is a subtype of `target` type.
    ///
    /// Stricter than assignability. Uses coinductive semantics for recursive types.
    /// Uses the context's TypeEnvironment for resolving type references and expanding Applications.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;
        let depth_exceeded = {
            let env = self.ctx.type_env.borrow();
            let mut checker = SubtypeChecker::with_resolver(self.ctx.types, &*env)
                .with_strict_null_checks(self.ctx.strict_null_checks);
            let result = checker.is_subtype_of(source, target);
            let depth_exceeded = checker.depth_exceeded;
            (result, depth_exceeded)
        };

        // Emit TS2589 if recursion depth was exceeded
        if depth_exceeded.1 {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
                diagnostic_codes::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
            );
        }

        depth_exceeded.0
    }

    /// Check if `source` type is a subtype of `target` type, resolving Ref types.
    ///
    /// Uses the provided TypeEnvironment to resolve type references.
    pub fn is_subtype_of_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
        env: &crate::solver::TypeEnvironment,
    ) -> bool {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::SubtypeChecker;
        let mut checker = SubtypeChecker::with_resolver(self.ctx.types, env)
            .with_strict_null_checks(self.ctx.strict_null_checks);
        let result = checker.is_subtype_of(source, target);
        let depth_exceeded = checker.depth_exceeded;

        // Emit TS2589 if recursion depth was exceeded
        if depth_exceeded {
            self.error_at_current_node(
                diagnostic_messages::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
                diagnostic_codes::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
            );
        }

        result
    }

    /// Check if two types are identical.
    ///
    /// O(1) operation - just compare TypeId values (structural interning).
    pub fn are_types_identical(&self, type1: TypeId, type2: TypeId) -> bool {
        type1 == type2
    }

    fn are_var_decl_types_compatible(&mut self, prev_type: TypeId, current_type: TypeId) -> bool {
        let prev_type = self
            .enum_symbol_from_value_type(prev_type)
            .and_then(|sym_id| self.enum_object_type(sym_id))
            .unwrap_or(prev_type);
        let current_type = self
            .enum_symbol_from_value_type(current_type)
            .and_then(|sym_id| self.enum_object_type(sym_id))
            .unwrap_or(current_type);

        if prev_type == current_type {
            return true;
        }
        if matches!(prev_type, TypeId::ERROR) || matches!(current_type, TypeId::ERROR) {
            return true;
        }
        self.ensure_application_symbols_resolved(prev_type);
        self.ensure_application_symbols_resolved(current_type);
        self.is_assignable_to(prev_type, current_type)
            && self.is_assignable_to(current_type, prev_type)
    }

    fn refine_var_decl_type(&self, prev_type: TypeId, current_type: TypeId) -> TypeId {
        if matches!(prev_type, TypeId::ANY | TypeId::ERROR)
            && !matches!(current_type, TypeId::ANY | TypeId::ERROR)
        {
            return current_type;
        }
        prev_type
    }

    /// Check if a type is assignable to a union of types.
    /// Uses the context's TypeEnvironment for resolving type references and expanding Applications.
    pub fn is_assignable_to_union(&self, source: TypeId, targets: &[TypeId]) -> bool {
        use crate::solver::CompatChecker;
        let env = self.ctx.type_env.borrow();
        let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
        checker.set_strict_null_checks(self.ctx.strict_null_checks);
        for &target in targets {
            if checker.is_assignable(source, target) {
                return true;
            }
        }
        false
    }

    /// Evaluate an Application type by resolving the base symbol and instantiating.
    ///
    /// This handles types like `Store<ExtractState<R>>` by:
    /// 1. Resolving the base type reference to get its body
    /// 2. Getting the type parameters
    /// 3. Instantiating the body with the provided type arguments
    /// 4. Recursively evaluating the result
    fn evaluate_application_type(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        let Some(TypeKey::Application(_)) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        if let Some(&cached) = self.ctx.application_eval_cache.get(&type_id) {
            return cached;
        }

        if !self.ctx.application_eval_set.insert(type_id) {
            // Recursion guard for self-referential mapped types.
            return type_id;
        }

        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.application_eval_set.remove(&type_id);
            return type_id;
        }
        *self.ctx.instantiation_depth.borrow_mut() += 1;

        let result = self.evaluate_application_type_inner(type_id);

        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        self.ctx.application_eval_set.remove(&type_id);
        self.ctx.application_eval_cache.insert(type_id, result);
        result
    }

    fn evaluate_application_type_inner(&mut self, type_id: TypeId) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey, TypeSubstitution, instantiate_type};

        let Some(TypeKey::Application(app_id)) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        let app = self.ctx.types.type_application(app_id);

        // Check if the base is a Ref
        let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(app.base) else {
            return type_id;
        };

        // Get the symbol's type (which is the body of the type alias/interface)
        let body_type = self.type_reference_symbol_type(SymbolId(sym_id));
        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return type_id;
        }

        // Get type parameters for this symbol
        let type_params = self.get_type_params_for_symbol(SymbolId(sym_id));
        if type_params.is_empty() {
            return body_type;
        }

        // Resolve type arguments so distributive conditionals can see unions.
        let evaluated_args: Vec<TypeId> = app
            .args
            .iter()
            .map(|&arg| self.evaluate_type_with_env(arg))
            .collect();

        // Create substitution and instantiate
        let substitution = TypeSubstitution::from_args(&type_params, &evaluated_args);
        let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);

        // Recursively evaluate in case the result contains more applications
        let result = self.evaluate_application_type(instantiated);

        // If the result is a Mapped type, try to evaluate it with symbol resolution
        let result = self.evaluate_mapped_type_with_resolution(result);

        // Evaluate meta-types (conditional, index access, keyof) with symbol resolution
        self.evaluate_type_with_env(result)
    }

    /// Evaluate a mapped type with symbol resolution.
    /// This handles cases like `{ [K in keyof Ref(sym)]: Template }` where the Ref
    /// needs to be resolved to get concrete keys.
    fn evaluate_mapped_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        let Some(TypeKey::Mapped(mapped_id)) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        if let Some(&cached) = self.ctx.mapped_eval_cache.get(&type_id) {
            return cached;
        }

        if !self.ctx.mapped_eval_set.insert(type_id) {
            return type_id;
        }

        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.mapped_eval_set.remove(&type_id);
            return type_id;
        }
        *self.ctx.instantiation_depth.borrow_mut() += 1;

        let result = self.evaluate_mapped_type_with_resolution_inner(type_id, mapped_id);

        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        self.ctx.mapped_eval_set.remove(&type_id);
        self.ctx.mapped_eval_cache.insert(type_id, result);
        result
    }

    fn evaluate_mapped_type_with_resolution_inner(
        &mut self,
        type_id: TypeId,
        mapped_id: crate::solver::MappedTypeId,
    ) -> TypeId {
        use crate::solver::{
            LiteralValue, PropertyInfo, TypeKey, TypeSubstitution, instantiate_type,
        };

        let mapped = self.ctx.types.mapped_type(mapped_id);

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);

        // Extract string literal keys
        let string_keys = self.extract_string_literal_keys(keys);
        if string_keys.is_empty() {
            // Can't evaluate - return original
            return type_id;
        }

        // Build the resulting object properties
        let mut properties = Vec::new();
        for key_name in string_keys {
            // Create the key literal type
            let key_literal = self
                .ctx
                .types
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));

            // Substitute the type parameter with the key
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            // Instantiate the template without recursively expanding nested applications.
            let property_type = instantiate_type(self.ctx.types, mapped.template, &subst);

            let optional = matches!(
                mapped.optional_modifier,
                Some(crate::solver::MappedModifier::Add)
            );
            let readonly = matches!(
                mapped.readonly_modifier,
                Some(crate::solver::MappedModifier::Add)
            );

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
            });
        }

        self.ctx.types.object(properties)
    }

    /// Evaluate a mapped type constraint with symbol resolution.
    /// Handles keyof Ref(sym) by resolving the Ref and getting its keys.
    fn evaluate_mapped_constraint_with_resolution(&mut self, constraint: TypeId) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{LiteralValue, SymbolRef, TypeKey};

        let Some(key) = self.ctx.types.lookup(constraint) else {
            return constraint;
        };

        match key {
            TypeKey::KeyOf(operand) => {
                // Evaluate the operand with symbol resolution
                let evaluated = self.evaluate_type_with_resolution(operand);
                self.get_keyof_type(evaluated)
            }
            TypeKey::Union(_) | TypeKey::Literal(_) => constraint,
            _ => constraint,
        }
    }

    /// Evaluate a type with symbol resolution (Refs resolved to their concrete types).
    fn evaluate_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        match key {
            TypeKey::Ref(SymbolRef(sym_id)) => self.type_reference_symbol_type(SymbolId(sym_id)),
            TypeKey::Application(_) => self.evaluate_application_type(type_id),
            _ => type_id,
        }
    }

    fn evaluate_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        use crate::solver::TypeEvaluator;

        self.ensure_application_symbols_resolved(type_id);

        let env = self.ctx.type_env.borrow();
        let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &*env);
        evaluator.evaluate(type_id)
    }

    fn resolve_global_interface_type(&mut self, name: &str) -> Option<TypeId> {
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        self.resolve_lib_type_by_name(name)
    }

    fn apply_function_interface_for_property_access(&mut self, type_id: TypeId) -> TypeId {
        let Some(function_type) = self.resolve_global_interface_type("Function") else {
            return type_id;
        };
        if function_type == TypeId::ANY
            || function_type == TypeId::ERROR
            || function_type == TypeId::UNKNOWN
        {
            return type_id;
        }
        self.ctx.types.intersection2(type_id, function_type)
    }

    fn resolve_type_for_property_access(&mut self, type_id: TypeId) -> TypeId {
        use rustc_hash::FxHashSet;

        self.ensure_application_symbols_resolved(type_id);

        let mut visited = FxHashSet::default();
        self.resolve_type_for_property_access_inner(type_id, &mut visited)
    }

    fn resolve_type_for_property_access_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        if !visited.insert(type_id) {
            return type_id;
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        match key {
            TypeKey::Ref(SymbolRef(sym_id)) => {
                let sym_id = SymbolId(sym_id);
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    if symbol.flags & symbol_flags::CLASS != 0
                        && symbol.flags & symbol_flags::MODULE != 0
                    {
                        if let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id) {
                            if let Some(class_node) = self.ctx.arena.get(class_idx) {
                                if let Some(class_data) = self.ctx.arena.get_class(class_node) {
                                    let ctor_type =
                                        self.get_class_constructor_type(class_idx, class_data);
                                    if ctor_type == type_id {
                                        return type_id;
                                    }
                                    return self.resolve_type_for_property_access_inner(
                                        ctor_type, visited,
                                    );
                                }
                            }
                        }
                    }
                }

                let resolved = self.type_reference_symbol_type(sym_id);
                if resolved == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(resolved, visited)
                }
            }
            TypeKey::TypeQuery(SymbolRef(sym_id)) => {
                let resolved = self.get_type_of_symbol(SymbolId(sym_id));
                if resolved == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(resolved, visited)
                }
            }
            TypeKey::Application(_) => {
                let evaluated = self.evaluate_application_type(type_id);
                if evaluated == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(evaluated, visited)
                }
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    if constraint == type_id {
                        type_id
                    } else {
                        self.resolve_type_for_property_access_inner(constraint, visited)
                    }
                } else {
                    type_id
                }
            }
            TypeKey::Conditional(_)
            | TypeKey::Mapped(_)
            | TypeKey::IndexAccess(_, _)
            | TypeKey::KeyOf(_) => {
                let evaluated = self.evaluate_type_with_env(type_id);
                if evaluated == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(evaluated, visited)
                }
            }
            TypeKey::Union(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.resolve_type_for_property_access_inner(member, visited))
                    .collect();
                self.ctx.types.union(resolved_members)
            }
            TypeKey::Intersection(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.resolve_type_for_property_access_inner(member, visited))
                    .collect();
                self.ctx.types.intersection(resolved_members)
            }
            TypeKey::ReadonlyType(inner) => {
                self.resolve_type_for_property_access_inner(inner, visited)
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                if let Some(constraint) = info.constraint {
                    self.resolve_type_for_property_access_inner(constraint, visited)
                } else {
                    type_id
                }
            }
            TypeKey::Function(_) | TypeKey::Callable(_) => {
                let expanded = self.apply_function_interface_for_property_access(type_id);
                if expanded == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(expanded, visited)
                }
            }
            _ => type_id,
        }
    }

    fn substitute_this_type(&mut self, type_id: TypeId, this_type: TypeId) -> TypeId {
        use rustc_hash::FxHashMap;

        let mut cache = FxHashMap::default();
        self.substitute_this_type_inner(type_id, this_type, &mut cache)
    }

    fn substitute_this_type_inner(
        &mut self,
        type_id: TypeId,
        this_type: TypeId,
        cache: &mut rustc_hash::FxHashMap<TypeId, TypeId>,
    ) -> TypeId {
        use crate::solver::{
            CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature,
            MappedType, ObjectShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeKey,
            TypeParamInfo, TypePredicate,
        };

        if type_id == this_type {
            return type_id;
        }

        if let Some(&cached) = cache.get(&type_id) {
            return cached;
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return type_id;
        };

        cache.insert(type_id, type_id);

        let result = match key {
            TypeKey::ThisType => this_type,
            TypeKey::Union(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let mut changed = false;
                let new_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let new_member = self.substitute_this_type_inner(member, this_type, cache);
                        if new_member != member {
                            changed = true;
                        }
                        new_member
                    })
                    .collect();
                if changed {
                    self.ctx.types.union(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Intersection(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                let mut changed = false;
                let new_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let new_member = self.substitute_this_type_inner(member, this_type, cache);
                        if new_member != member {
                            changed = true;
                        }
                        new_member
                    })
                    .collect();
                if changed {
                    self.ctx.types.intersection(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Array(elem) => {
                let new_elem = self.substitute_this_type_inner(elem, this_type, cache);
                if new_elem == elem {
                    type_id
                } else {
                    self.ctx.types.array(new_elem)
                }
            }
            TypeKey::Tuple(elems_id) => {
                let elems = self.ctx.types.tuple_list(elems_id);
                let mut changed = false;
                let new_elems: Vec<TupleElement> = elems
                    .iter()
                    .map(|elem| {
                        let new_type =
                            self.substitute_this_type_inner(elem.type_id, this_type, cache);
                        if new_type != elem.type_id {
                            changed = true;
                        }
                        TupleElement {
                            type_id: new_type,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.tuple(new_elems)
                } else {
                    type_id
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                let mut changed = false;
                let props: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let new_type =
                            self.substitute_this_type_inner(prop.type_id, this_type, cache);
                        let new_write =
                            self.substitute_this_type_inner(prop.write_type, this_type, cache);
                        if new_type != prop.type_id || new_write != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: new_type,
                            write_type: new_write,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.object(props)
                } else {
                    type_id
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                let mut changed = false;
                let props: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let new_type =
                            self.substitute_this_type_inner(prop.type_id, this_type, cache);
                        let new_write =
                            self.substitute_this_type_inner(prop.write_type, this_type, cache);
                        if new_type != prop.type_id || new_write != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: new_type,
                            write_type: new_write,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                        }
                    })
                    .collect();
                let string_index = shape.string_index.as_ref().map(|idx| {
                    let new_key = self.substitute_this_type_inner(idx.key_type, this_type, cache);
                    let new_value =
                        self.substitute_this_type_inner(idx.value_type, this_type, cache);
                    if new_key != idx.key_type || new_value != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type: new_key,
                        value_type: new_value,
                        readonly: idx.readonly,
                    }
                });
                let number_index = shape.number_index.as_ref().map(|idx| {
                    let new_key = self.substitute_this_type_inner(idx.key_type, this_type, cache);
                    let new_value =
                        self.substitute_this_type_inner(idx.value_type, this_type, cache);
                    if new_key != idx.key_type || new_value != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type: new_key,
                        value_type: new_value,
                        readonly: idx.readonly,
                    }
                });
                if changed {
                    self.ctx.types.object_with_index(ObjectShape {
                        properties: props,
                        string_index,
                        number_index,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                let mut changed = false;
                let params: Vec<ParamInfo> = shape
                    .params
                    .iter()
                    .map(|param| {
                        let new_type =
                            self.substitute_this_type_inner(param.type_id, this_type, cache);
                        if new_type != param.type_id {
                            changed = true;
                        }
                        ParamInfo {
                            name: param.name,
                            type_id: new_type,
                            optional: param.optional,
                            rest: param.rest,
                        }
                    })
                    .collect();
                let this_param = shape.this_type.map(|this_param| {
                    let new_type = self.substitute_this_type_inner(this_param, this_type, cache);
                    if new_type != this_param {
                        changed = true;
                    }
                    new_type
                });
                let return_type =
                    self.substitute_this_type_inner(shape.return_type, this_type, cache);
                if return_type != shape.return_type {
                    changed = true;
                }
                let type_predicate = shape.type_predicate.as_ref().map(|pred| {
                    let new_pred_type = pred.type_id.map(|pred_type| {
                        let new_type = self.substitute_this_type_inner(pred_type, this_type, cache);
                        if new_type != pred_type {
                            changed = true;
                        }
                        new_type
                    });
                    if new_pred_type != pred.type_id {
                        changed = true;
                    }
                    TypePredicate {
                        asserts: pred.asserts,
                        target: pred.target.clone(),
                        type_id: new_pred_type,
                    }
                });
                if changed {
                    self.ctx.types.function(FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: this_param,
                        return_type,
                        type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let mut changed = false;
                let mut map_signature = |sig: &CallSignature,
                                         this_type: TypeId,
                                         cache: &mut rustc_hash::FxHashMap<TypeId, TypeId>,
                                         ctx: &mut ThinCheckerState|
                 -> CallSignature {
                    let mut local_changed = false;
                    let params: Vec<ParamInfo> = sig
                        .params
                        .iter()
                        .map(|param| {
                            let new_type =
                                ctx.substitute_this_type_inner(param.type_id, this_type, cache);
                            if new_type != param.type_id {
                                local_changed = true;
                            }
                            ParamInfo {
                                name: param.name,
                                type_id: new_type,
                                optional: param.optional,
                                rest: param.rest,
                            }
                        })
                        .collect();
                    let this_param = sig.this_type.map(|this_param| {
                        let new_type = ctx.substitute_this_type_inner(this_param, this_type, cache);
                        if new_type != this_param {
                            local_changed = true;
                        }
                        new_type
                    });
                    let return_type =
                        ctx.substitute_this_type_inner(sig.return_type, this_type, cache);
                    if return_type != sig.return_type {
                        local_changed = true;
                    }
                    let type_predicate = sig.type_predicate.as_ref().map(|pred| {
                        let new_pred_type = pred.type_id.map(|pred_type| {
                            let new_type =
                                ctx.substitute_this_type_inner(pred_type, this_type, cache);
                            if new_type != pred_type {
                                local_changed = true;
                            }
                            new_type
                        });
                        if new_pred_type != pred.type_id {
                            local_changed = true;
                        }
                        TypePredicate {
                            asserts: pred.asserts,
                            target: pred.target.clone(),
                            type_id: new_pred_type,
                        }
                    });
                    if local_changed {
                        changed = true;
                    }
                    CallSignature {
                        type_params: sig.type_params.clone(),
                        params,
                        this_type: this_param,
                        return_type,
                        type_predicate,
                    }
                };
                let call_signatures: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| map_signature(sig, this_type, cache, self))
                    .collect();
                let construct_signatures: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| map_signature(sig, this_type, cache, self))
                    .collect();
                let properties: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let new_type =
                            self.substitute_this_type_inner(prop.type_id, this_type, cache);
                        let new_write =
                            self.substitute_this_type_inner(prop.write_type, this_type, cache);
                        if new_type != prop.type_id || new_write != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: new_type,
                            write_type: new_write,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.callable(CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties,
                        string_index: shape.string_index.clone(),
                        number_index: shape.number_index.clone(),
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                let mut changed = false;
                let check_type = self.substitute_this_type_inner(cond.check_type, this_type, cache);
                if check_type != cond.check_type {
                    changed = true;
                }
                let extends_type =
                    self.substitute_this_type_inner(cond.extends_type, this_type, cache);
                if extends_type != cond.extends_type {
                    changed = true;
                }
                let true_type = self.substitute_this_type_inner(cond.true_type, this_type, cache);
                if true_type != cond.true_type {
                    changed = true;
                }
                let false_type = self.substitute_this_type_inner(cond.false_type, this_type, cache);
                if false_type != cond.false_type {
                    changed = true;
                }
                if changed {
                    self.ctx.types.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                let mut changed = false;
                let type_param = TypeParamInfo {
                    name: mapped.type_param.name,
                    constraint: mapped.type_param.constraint.map(|constraint| {
                        self.substitute_this_type_inner(constraint, this_type, cache)
                    }),
                    default: mapped
                        .type_param
                        .default
                        .map(|default| self.substitute_this_type_inner(default, this_type, cache)),
                };
                if type_param.constraint != mapped.type_param.constraint
                    || type_param.default != mapped.type_param.default
                {
                    changed = true;
                }
                let constraint =
                    self.substitute_this_type_inner(mapped.constraint, this_type, cache);
                if constraint != mapped.constraint {
                    changed = true;
                }
                let name_type = mapped
                    .name_type
                    .map(|name_type| self.substitute_this_type_inner(name_type, this_type, cache));
                if name_type != mapped.name_type {
                    changed = true;
                }
                let template = self.substitute_this_type_inner(mapped.template, this_type, cache);
                if template != mapped.template {
                    changed = true;
                }
                if changed {
                    self.ctx.types.mapped(MappedType {
                        type_param,
                        constraint,
                        name_type,
                        template,
                        readonly_modifier: mapped.readonly_modifier,
                        optional_modifier: mapped.optional_modifier,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::IndexAccess(obj, idx) => {
                let new_obj = self.substitute_this_type_inner(obj, this_type, cache);
                let new_idx = self.substitute_this_type_inner(idx, this_type, cache);
                if new_obj == obj && new_idx == idx {
                    type_id
                } else {
                    self.ctx
                        .types
                        .intern(TypeKey::IndexAccess(new_obj, new_idx))
                }
            }
            TypeKey::KeyOf(inner) => {
                let new_inner = self.substitute_this_type_inner(inner, this_type, cache);
                if new_inner == inner {
                    type_id
                } else {
                    self.ctx.types.intern(TypeKey::KeyOf(new_inner))
                }
            }
            TypeKey::ReadonlyType(inner) => {
                let new_inner = self.substitute_this_type_inner(inner, this_type, cache);
                if new_inner == inner {
                    type_id
                } else {
                    self.ctx.types.intern(TypeKey::ReadonlyType(new_inner))
                }
            }
            TypeKey::TemplateLiteral(template_id) => {
                let spans = self.ctx.types.template_list(template_id);
                let mut changed = false;
                let new_spans: Vec<TemplateSpan> = spans
                    .iter()
                    .map(|span| match span {
                        TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                        TemplateSpan::Type(span_type) => {
                            let new_type =
                                self.substitute_this_type_inner(*span_type, this_type, cache);
                            if new_type != *span_type {
                                changed = true;
                            }
                            TemplateSpan::Type(new_type)
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.template_literal(new_spans)
                } else {
                    type_id
                }
            }
            TypeKey::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                let mut changed = false;
                let base = self.substitute_this_type_inner(app.base, this_type, cache);
                if base != app.base {
                    changed = true;
                }
                let args: Vec<TypeId> = app
                    .args
                    .iter()
                    .map(|&arg| {
                        let new_arg = self.substitute_this_type_inner(arg, this_type, cache);
                        if new_arg != arg {
                            changed = true;
                        }
                        new_arg
                    })
                    .collect();
                if changed {
                    self.ctx.types.application(base, args)
                } else {
                    type_id
                }
            }
            TypeKey::TypeParameter(info) => {
                let constraint = info.constraint.map(|constraint| {
                    self.substitute_this_type_inner(constraint, this_type, cache)
                });
                let default = info
                    .default
                    .map(|default| self.substitute_this_type_inner(default, this_type, cache));
                if constraint == info.constraint && default == info.default {
                    type_id
                } else {
                    self.ctx.types.intern(TypeKey::TypeParameter(TypeParamInfo {
                        name: info.name,
                        constraint,
                        default,
                    }))
                }
            }
            TypeKey::Infer(info) => {
                let constraint = info.constraint.map(|constraint| {
                    self.substitute_this_type_inner(constraint, this_type, cache)
                });
                let default = info
                    .default
                    .map(|default| self.substitute_this_type_inner(default, this_type, cache));
                if constraint == info.constraint && default == info.default {
                    type_id
                } else {
                    self.ctx.types.intern(TypeKey::Infer(TypeParamInfo {
                        name: info.name,
                        constraint,
                        default,
                    }))
                }
            }
            _ => type_id,
        };

        cache.insert(type_id, result);
        result
    }

    /// Get keyof a type - extract the keys of an object type.
    fn get_keyof_type(&self, operand: TypeId) -> TypeId {
        use crate::solver::{LiteralValue, TypeKey};

        let Some(key) = self.ctx.types.lookup(operand) else {
            return TypeId::NEVER;
        };

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return TypeId::NEVER;
                }
                let key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        self.ctx
                            .types
                            .intern(TypeKey::Literal(LiteralValue::String(p.name)))
                    })
                    .collect();
                self.ctx.types.union(key_types)
            }
            _ => TypeId::NEVER,
        }
    }

    /// Extract string literal keys from a union or single literal type.
    fn extract_string_literal_keys(&self, type_id: TypeId) -> Vec<crate::interner::Atom> {
        use crate::solver::{LiteralValue, TypeKey};

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return Vec::new();
        };

        match key {
            TypeKey::Literal(LiteralValue::String(name)) => vec![name],
            TypeKey::Union(list_id) => {
                let members = self.ctx.types.type_list(list_id);
                members
                    .iter()
                    .filter_map(|&member| {
                        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
                            self.ctx.types.lookup(member)
                        {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Ensure all symbols referenced in Application types are resolved in the type_env.
    /// This walks the type structure and calls get_type_of_symbol for any Application base symbols.
    fn ensure_application_symbols_resolved(&mut self, type_id: TypeId) {
        use crate::solver::TypeKey;
        use std::collections::HashSet;

        let mut visited: HashSet<TypeId> = HashSet::new();
        self.ensure_application_symbols_resolved_inner(type_id, &mut visited);
    }

    fn insert_type_env_symbol(&mut self, sym_id: crate::binder::SymbolId, resolved: TypeId) {
        use crate::solver::SymbolRef;

        if resolved == TypeId::ANY || resolved == TypeId::ERROR {
            return;
        }

        let type_params = self.get_type_params_for_symbol(sym_id);
        let mut env = self.ctx.type_env.borrow_mut();
        if type_params.is_empty() {
            env.insert(SymbolRef(sym_id.0), resolved);
        } else {
            env.insert_with_params(SymbolRef(sym_id.0), resolved, type_params);
        }
    }

    fn ensure_application_symbols_resolved_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) {
        use crate::binder::SymbolId;
        use crate::solver::{SymbolRef, TypeKey};

        if !visited.insert(type_id) {
            return;
        }

        let Some(key) = self.ctx.types.lookup(type_id) else {
            return;
        };

        match key {
            TypeKey::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);

                // If the base is a Ref, resolve the symbol
                if let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(app.base) {
                    let sym_id = SymbolId(sym_id);
                    let resolved = self.type_reference_symbol_type(sym_id);
                    self.insert_type_env_symbol(sym_id, resolved);
                }

                // Recursively process base and args
                self.ensure_application_symbols_resolved_inner(app.base, visited);
                for &arg in &app.args {
                    self.ensure_application_symbols_resolved_inner(arg, visited);
                }
            }
            TypeKey::Ref(SymbolRef(sym_id)) => {
                let sym_id = SymbolId(sym_id);
                let resolved = self.type_reference_symbol_type(sym_id);
                self.insert_type_env_symbol(sym_id, resolved);
            }
            TypeKey::TypeParameter(param) => {
                if let Some(constraint) = param.constraint {
                    self.ensure_application_symbols_resolved_inner(constraint, visited);
                }
                if let Some(default) = param.default {
                    self.ensure_application_symbols_resolved_inner(default, visited);
                }
            }
            TypeKey::Union(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                for member in members.iter() {
                    self.ensure_application_symbols_resolved_inner(*member, visited);
                }
            }
            TypeKey::Intersection(members_id) => {
                let members = self.ctx.types.type_list(members_id);
                for member in members.iter() {
                    self.ensure_application_symbols_resolved_inner(*member, visited);
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                for type_param in shape.type_params.iter() {
                    if let Some(constraint) = type_param.constraint {
                        self.ensure_application_symbols_resolved_inner(constraint, visited);
                    }
                    if let Some(default) = type_param.default {
                        self.ensure_application_symbols_resolved_inner(default, visited);
                    }
                }
                for param in shape.params.iter() {
                    self.ensure_application_symbols_resolved_inner(param.type_id, visited);
                }
                if let Some(this_type) = shape.this_type {
                    self.ensure_application_symbols_resolved_inner(this_type, visited);
                }
                self.ensure_application_symbols_resolved_inner(shape.return_type, visited);
                if let Some(predicate) = &shape.type_predicate {
                    if let Some(type_id) = predicate.type_id {
                        self.ensure_application_symbols_resolved_inner(type_id, visited);
                    }
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for type_param in sig.type_params.iter() {
                        if let Some(constraint) = type_param.constraint {
                            self.ensure_application_symbols_resolved_inner(constraint, visited);
                        }
                        if let Some(default) = type_param.default {
                            self.ensure_application_symbols_resolved_inner(default, visited);
                        }
                    }
                    for param in sig.params.iter() {
                        self.ensure_application_symbols_resolved_inner(param.type_id, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.ensure_application_symbols_resolved_inner(this_type, visited);
                    }
                    self.ensure_application_symbols_resolved_inner(sig.return_type, visited);
                    if let Some(predicate) = &sig.type_predicate {
                        if let Some(type_id) = predicate.type_id {
                            self.ensure_application_symbols_resolved_inner(type_id, visited);
                        }
                    }
                }
                for prop in shape.properties.iter() {
                    self.ensure_application_symbols_resolved_inner(prop.type_id, visited);
                }
            }
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.ensure_application_symbols_resolved_inner(prop.type_id, visited);
                }
                if let Some(ref idx) = shape.string_index {
                    self.ensure_application_symbols_resolved_inner(idx.value_type, visited);
                }
                if let Some(ref idx) = shape.number_index {
                    self.ensure_application_symbols_resolved_inner(idx.value_type, visited);
                }
            }
            TypeKey::Array(elem) => {
                self.ensure_application_symbols_resolved_inner(elem, visited);
            }
            TypeKey::Tuple(elems_id) => {
                let elems = self.ctx.types.tuple_list(elems_id);
                for elem in elems.iter() {
                    self.ensure_application_symbols_resolved_inner(elem.type_id, visited);
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.ensure_application_symbols_resolved_inner(cond.check_type, visited);
                self.ensure_application_symbols_resolved_inner(cond.extends_type, visited);
                self.ensure_application_symbols_resolved_inner(cond.true_type, visited);
                self.ensure_application_symbols_resolved_inner(cond.false_type, visited);
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                self.ensure_application_symbols_resolved_inner(mapped.constraint, visited);
                self.ensure_application_symbols_resolved_inner(mapped.template, visited);
                if let Some(name_type) = mapped.name_type {
                    self.ensure_application_symbols_resolved_inner(name_type, visited);
                }
            }
            TypeKey::ReadonlyType(inner) => {
                self.ensure_application_symbols_resolved_inner(inner, visited);
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.ensure_application_symbols_resolved_inner(obj, visited);
                self.ensure_application_symbols_resolved_inner(idx, visited);
            }
            TypeKey::KeyOf(inner) => {
                self.ensure_application_symbols_resolved_inner(inner, visited);
            }
            _ => {}
        }
    }

    /// Create a TypeEnvironment populated with resolved symbol types.
    ///
    /// This can be passed to `is_assignable_to_with_env` for type checking
    /// that needs to resolve type references.
    pub fn build_type_environment(&mut self) -> crate::solver::TypeEnvironment {
        use crate::solver::{SymbolRef, TypeEnvironment};

        let mut env = TypeEnvironment::new();

        // Collect all unique symbols from node_symbols map
        let symbols: Vec<SymbolId> = self
            .ctx
            .binder
            .node_symbols
            .values()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Resolve each symbol and add to the environment
        for sym_id in symbols {
            // Get the type for this symbol
            let type_id = self.get_type_of_symbol(sym_id);
            if type_id != TypeId::ANY && type_id != TypeId::ERROR {
                // Get type parameters if this is a generic type
                let type_params = self.get_type_params_for_symbol(sym_id);
                if type_params.is_empty() {
                    env.insert(SymbolRef(sym_id.0), type_id);
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), type_id, type_params);
                }
            }
        }

        env
    }

    /// Get type parameters for a symbol (for generic type aliases and interfaces).
    fn get_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<crate::solver::TypeParamInfo> {
        if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            if !std::ptr::eq(symbol_arena.as_ref(), self.ctx.arena) {
                let mut checker = ThinCheckerState::new(
                    symbol_arena.as_ref(),
                    self.ctx.binder,
                    self.ctx.types,
                    self.ctx.file_name.clone(),
                    self.ctx.no_implicit_any, // use current strict mode setting
                );
                return checker.get_type_params_for_symbol(sym_id);
            }
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return Vec::new();
        };

        let flags = symbol.flags;
        let value_decl = symbol.value_declaration;

        // Type alias - get type parameters from declaration
        if flags & symbol_flags::TYPE_ALIAS != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                symbol
                    .declarations
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none() {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                        let (params, updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        self.pop_type_parameters(updates);
                        return params;
                    }
                }
            }
        }

        // Class - get type parameters from declaration
        if flags & symbol_flags::CLASS != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                symbol
                    .declarations
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none() {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(class) = self.ctx.arena.get_class(node) {
                        let (params, updates) = self.push_type_parameters(&class.type_parameters);
                        self.pop_type_parameters(updates);
                        return params;
                    }
                }
            }
        }

        // Interface - get type parameters from first declaration
        if flags & symbol_flags::INTERFACE != 0 {
            let decl_idx = if !value_decl.is_none() {
                value_decl
            } else {
                symbol
                    .declarations
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE)
            };
            if !decl_idx.is_none() {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(iface) = self.ctx.arena.get_interface(node) {
                        let (params, updates) = self.push_type_parameters(&iface.type_parameters);
                        self.pop_type_parameters(updates);
                        return params;
                    }
                }
            }
        }

        Vec::new()
    }

    /// Create a union type from multiple types.
    ///
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        self.ctx.types.union(types)
    }

    /// Create an intersection type from multiple types.
    ///
    /// Automatically normalizes: flattens nested intersections, deduplicates, sorts.
    pub fn get_intersection_type(&self, types: Vec<TypeId>) -> TypeId {
        self.ctx.types.intersection(types)
    }

    // =========================================================================
    // Type Narrowing (uses solver::NarrowingContext)
    // =========================================================================

    /// Narrow a type by a typeof guard.
    ///
    /// Example: `typeof x === "string"` narrows `string | number` to `string`.
    pub fn narrow_by_typeof(&self, source: TypeId, typeof_result: &str) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_typeof(source, typeof_result)
    }

    /// Narrow a type by excluding a typeof guard.
    ///
    /// Example: `typeof x !== "string"` narrows `string | number` to `number`.
    pub fn narrow_by_typeof_negation(&self, source: TypeId, typeof_result: &str) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);

        // Get the target type for this typeof result
        let target = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "object" => TypeId::OBJECT,
            "function" => return ctx.narrow_excluding_function(source),
            _ => return source,
        };

        ctx.narrow_excluding_type(source, target)
    }

    /// Narrow a discriminated union by a discriminant property check.
    ///
    /// Example: `action.type === "add"` narrows `{ type: "add" } | { type: "remove" }`
    /// to `{ type: "add" }`.
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        literal_value: TypeId,
    ) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_discriminant(union_type, property_name, literal_value)
    }

    /// Narrow a discriminated union by excluding a discriminant value.
    ///
    /// Example: `action.type !== "add"` narrows the union to exclude the "add" variant.
    pub fn narrow_by_excluding_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        excluded_value: TypeId,
    ) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_by_excluding_discriminant(union_type, property_name, excluded_value)
    }
    /// Find discriminant properties in a union type.
    ///
    /// Returns information about properties that uniquely identify each union variant.
    pub fn find_discriminants(&self, union_type: TypeId) -> Vec<crate::solver::DiscriminantInfo> {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.find_discriminants(union_type)
    }

    /// Narrow a type to include only members assignable to target.
    pub fn narrow_to_type(&self, source: TypeId, target: TypeId) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_to_type(source, target)
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source: TypeId, excluded: TypeId) -> TypeId {
        use crate::solver::NarrowingContext;
        let ctx = NarrowingContext::new(self.ctx.types);
        ctx.narrow_excluding_type(source, excluded)
    }

    // =========================================================================
    // Type Node Resolution
    // =========================================================================

    /// Get type from a type node.
    ///
    /// Uses compile-time constant TypeIds for intrinsic types (O(1) lookup).
    /// Delegates to TypeLowering for complex types (union, intersection, array, etc.).
    /// Also validates that type references exist and reports errors.
    pub fn get_type_from_type_node(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::TypeLowering;

        // First check if this is a type that needs special handling with binder resolution
        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                // Validate the type reference exists before lowering
                return self.get_type_from_type_reference(idx);
            }
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                // Handle typeof X - need to resolve symbol properly via binder
                return self.get_type_from_type_query(idx);
            }
            if node.kind == syntax_kind_ext::UNION_TYPE {
                // Handle union types specially to ensure nested typeof expressions
                // are resolved via binder (for abstract class detection)
                return self.get_type_from_union_type(idx);
            }
            if node.kind == syntax_kind_ext::TYPE_LITERAL {
                // Type literals should use checker resolution so type parameters resolve correctly.
                return self.get_type_from_type_literal(idx);
            }
        }

        // Emit missing-name diagnostics for nested type references before lowering.
        self.check_type_for_missing_names(idx);

        // Use TypeLowering which handles all type nodes
        let type_param_bindings = self.get_type_param_bindings();
        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);
        lowering.lower_type(idx)
    }

    // =========================================================================
    // Source Location Tracking & Solver Diagnostics
    // =========================================================================

    /// Get a source location for a node.
    pub fn get_source_location(&self, idx: NodeIndex) -> Option<crate::solver::SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(crate::solver::SourceLocation::new(
            self.ctx.file_name.as_str(),
            node.pos,
            node.end,
        ))
    }

    /// Report a type not assignable error using solver diagnostics with source tracking.
    ///
    /// This is the basic error that just says "Type X is not assignable to Y".
    /// For detailed errors with elaboration (e.g., "property 'x' is missing"),
    /// use `error_type_not_assignable_with_reason_at` instead.
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        // SELECTIVE DIAGNOSTIC SUPPRESSION (2025-01-15 - Task 8 Pattern 1)
        //
        // When source or target type IS ERROR, suppress the TS2322 emission.
        // This prevents unhelpful errors like "Type 'error' is not assignable to type 'string'".
        //
        // Rationale:
        // 1. When a type resolves to ERROR, it means the symbol couldn't be resolved (TS2304)
        // 2. Emitting TS2322 for "Type 'error' is not assignable" provides no additional value
        // 3. TypeScript doesn't emit these errors - it only reports the resolution failure
        // 4. This fixes 7 out of 10 false positive test files (Pattern 1 in Task 8)
        //
        // The Worker 11 change removed all ERROR suppression to fix missing TS2322 errors,
        // but that was too broad. We need to be more selective:
        // - Suppress when source/target IS ERROR (can't provide useful error message)
        // - Don't suppress when source/target CONTAINS ERROR (e.g., union with error member)
        //
        // See: TASK_8_TEST_FAILURES.md Pattern 1 for full investigation details.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return;
        }

        // Additional suppression for ANY types - ANY should be assignable to and from any type
        // This matches TypeScript's behavior where any bypasses type checking
        if source == TypeId::ANY || target == TypeId::ANY {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.type_not_assignable(source, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a type not assignable error with detailed elaboration.
    ///
    /// This method uses the solver's "explain" API to determine WHY the types
    /// are incompatible (e.g., missing property, incompatible property types,
    /// etc.) and produces a richer diagnostic with that information.
    ///
    /// **Architecture Note**: This follows the "Check Fast, Explain Slow" pattern.
    /// The `is_assignable_to` check is fast (boolean). This explain call is slower
    /// but produces better error messages. Only call this after a failed check.
    pub fn error_type_not_assignable_with_reason_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use crate::solver::{CompatChecker, TypeFormatter};

        // SELECTIVE DIAGNOSTIC SUPPRESSION (2025-01-15 - Task 8 Pattern 1)
        //
        // When source or target type IS ERROR, suppress the TS2322 emission.
        // This prevents unhelpful errors like "Type 'error' is not assignable to type 'string'".
        //
        // Rationale:
        // 1. When a type resolves to ERROR, it means the symbol couldn't be resolved (TS2304)
        // 2. Emitting TS2322 for "Type 'error' is not assignable" provides no additional value
        // 3. TypeScript doesn't emit these errors - it only reports the resolution failure
        // 4. This fixes 7 out of 10 false positive test files (Pattern 1 in Task 8)
        //
        // The Worker 11 change removed all ERROR suppression to fix missing TS2322 errors,
        // but that was too broad. We need to be more selective:
        // - Suppress when source/target IS ERROR (can't provide useful error message)
        // - Don't suppress when source/target CONTAINS ERROR (e.g., union with error member)
        //
        // See: TASK_8_TEST_FAILURES.md Pattern 1 for full investigation details.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return;
        }

        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch(source, target, None)
        {
            self.error_constructor_accessibility_not_assignable(
                source,
                target,
                source_level,
                target_level,
                idx,
            );
            return;
        }

        // Check for private brand mismatch and generate specific error message
        if let Some(detail) = self.private_brand_mismatch_error(source, target) {
            use crate::checker::types::diagnostics::{
                diagnostic_codes, diagnostic_messages, format_message,
            };

            let Some(loc) = self.get_source_location(idx) else {
                return;
            };

            let source_type = self.format_type(source);
            let target_type = self.format_type(target);
            let message = format_message(
                diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                &[&source_type, &target_type],
            );

            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
            )
            .with_related(self.ctx.file_name.clone(), loc.start, loc.length(), detail);

            self.ctx.diagnostics.push(diag);
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        // Use the solver's explain API to get the detailed reason
        let mut checker = CompatChecker::new(self.ctx.types);
        let reason = checker.explain_failure(source, target);

        match reason {
            Some(failure_reason) => {
                // Convert the reason to a PendingDiagnostic with elaboration
                let pending = failure_reason.to_diagnostic(source, target).with_span(
                    crate::solver::SourceSpan::new(
                        self.ctx.file_name.as_str(),
                        loc.start,
                        loc.length(),
                    ),
                );

                // Render the pending diagnostic to a TypeDiagnostic
                let mut formatter =
                    TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols);
                let type_diag = formatter.render(&pending);

                // Convert to checker diagnostic and add
                self.ctx
                    .diagnostics
                    .push(type_diag.to_checker_diagnostic(&self.ctx.file_name));
            }
            None => {
                // Fallback: shouldn't happen if called after a failed check,
                // but just use the basic error message
                self.error_type_not_assignable_at(source, target, idx);
            }
        }
    }

    fn error_constructor_accessibility_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_level: Option<MemberAccessLevel>,
        target_level: Option<MemberAccessLevel>,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let source_type = self.format_type(source);
        let target_type = self.format_type(target);
        let message = format_message(
            diagnostic_messages::TYPE_NOT_ASSIGNABLE,
            &[&source_type, &target_type],
        );
        let detail = format!(
            "Cannot assign a '{}' constructor type to a '{}' constructor type.",
            Self::constructor_access_name(source_level),
            Self::constructor_access_name(target_level),
        );

        let diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message,
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
        )
        .with_related(self.ctx.file_name.clone(), loc.start, loc.length(), detail);
        self.ctx.diagnostics.push(diag);
    }

    /// Report a property missing error using solver diagnostics with source tracking.
    pub fn error_property_missing_at(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.property_missing(prop_name, source, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a property not exist error using solver diagnostics with source tracking.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.property_not_exist(prop_name, type_id, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report an argument not assignable error using solver diagnostics with source tracking.
    pub fn error_argument_not_assignable_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag =
                builder.argument_not_assignable(arg_type, param_type, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a cannot find name error using solver diagnostics with source tracking.
    pub fn error_cannot_find_name_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.cannot_find_name(name, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report error 2552: Cannot find name 'X'. Did you mean 'Y'?
    pub fn error_cannot_find_name_did_you_mean_at(
        &mut self,
        name: &str,
        suggestion: &str,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Cannot find name '{}'. Did you mean '{}'?",
                name, suggestion
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2662: Cannot find name 'X'. Did you mean the static member 'C.X'?
    pub fn error_cannot_find_name_static_member_at(
        &mut self,
        name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Cannot find name '{}'. Did you mean the static member '{}.{}'?",
                name, class_name, name
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_STATIC,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Check if two symbol declarations can merge (for TS2403 checking).
    /// Returns true if the declarations are mergeable and should NOT trigger TS2403.
    fn can_merge_symbols(&self, existing_flags: u32, new_flags: u32) -> bool {
        // Interface can merge with interface
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        // Class can merge with interface
        if ((existing_flags & symbol_flags::CLASS) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
            || ((existing_flags & symbol_flags::INTERFACE) != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        // Namespace/module can merge with namespace/module
        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Namespace can merge with class, function, or enum
        if (existing_flags & symbol_flags::MODULE) != 0 {
            if (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
            {
                return true;
            }
        }
        if (new_flags & symbol_flags::MODULE) != 0 {
            if (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
            {
                return true;
            }
        }

        // Function overloads
        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        false
    }

    /// Report error 2403: Subsequent variable declarations must have the same type.
    pub fn error_subsequent_variable_declaration(
        &mut self,
        name: &str,
        prev_type: TypeId,
        current_type: TypeId,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        if let Some(loc) = self.get_source_location(idx) {
            let prev_type_str = self.format_type(prev_type);
            let current_type_str = self.format_type(current_type);
            let message = format!(
                "Subsequent variable declarations must have the same type. Variable '{}' must be of type '{}', but here has type '{}'.",
                name, prev_type_str, current_type_str
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_SAME_TYPE,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2715: Abstract property 'X' in class 'C' cannot be accessed in the constructor.
    pub fn error_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Abstract property '{}' in class '{}' cannot be accessed in the constructor.",
                prop_name, class_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ABSTRACT_PROPERTY_IN_CONSTRUCTOR,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Check if a node is a `this` expression.
    fn is_this_expression(&self, idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(node) = self.ctx.arena.get(idx) {
            node.kind == SyntaxKind::ThisKeyword as u16
        } else {
            false
        }
    }

    fn is_global_this_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        ident.escaped_text == "globalThis"
    }

    fn resolve_global_value_symbol(&self, name: &str) -> Option<SymbolId> {
        self.ctx.binder.file_locals.get(name)
    }

    fn is_known_global_value_name(&self, name: &str) -> bool {
        matches!(
            name,
            "console"
                | "Math"
                | "JSON"
                | "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Function"
                | "Date"
                | "RegExp"
                | "Error"
                | "Promise"
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "WeakRef"
                | "Proxy"
                | "Reflect"
                | "globalThis"
                | "window"
                | "document"
                | "exports"
                | "module"
                | "require"
                | "__dirname"
                | "__filename"
                | "FinalizationRegistry"
                | "BigInt"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "BigInt64Array"
                | "BigUint64Array"
                | "Intl"
                | "Atomics"
                | "WebAssembly"
                | "Iterator"
                | "AsyncIterator"
                | "Generator"
                | "AsyncGenerator"
                | "URL"
                | "URLSearchParams"
                | "Headers"
                | "Request"
                | "Response"
                | "FormData"
                | "Blob"
                | "File"
                | "ReadableStream"
                | "WritableStream"
                | "TransformStream"
                | "TextEncoder"
                | "TextDecoder"
                | "AbortController"
                | "AbortSignal"
                | "fetch"
                | "setTimeout"
                | "setInterval"
                | "clearTimeout"
                | "clearInterval"
                | "queueMicrotask"
                | "structuredClone"
                | "atob"
                | "btoa"
                | "performance"
                | "crypto"
                | "navigator"
                | "location"
                | "history"
                | "exports"
        )
    }

    fn is_known_global_type_name(&self, name: &str) -> bool {
        matches!(
            name,
            "Object"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Function"
                | "Promise"
                | "PromiseLike"
                | "PromiseConstructor"
                | "PromiseConstructorLike"
                | "Awaited"
                | "Array"
                | "ReadonlyArray"
                | "ArrayLike"
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "WeakRef"
                | "Date"
                | "RegExp"
                | "RegExpExecArray"
                | "Partial"
                | "Required"
                | "Readonly"
                | "Record"
                | "Pick"
                | "Omit"
                | "Iterator"
                | "Iterable"
                | "AsyncIterator"
                | "AsyncIterable"
                | "Generator"
                | "AsyncGenerator"
                | "NonNullable"
                | "Extract"
                | "ThisType"
                | "PropertyKey"
                | "PropertyDescriptor"
                | "Element"
                | "HTMLElement"
                | "Document"
                | "Window"
                | "Event"
                | "NodeList"
                | "NodeListOf"
                | "Error"
                | "TypeError"
                | "RangeError"
                | "EvalError"
                | "URIError"
                | "ReferenceError"
                | "SyntaxError"
                // Primitive types (lowercase)
                | "number"
                | "string"
                | "boolean"
                | "void"
                | "null"
                | "undefined"
                | "never"
                | "unknown"
                | "any"
        )
    }

    fn resolve_global_this_property_type(&mut self, name: &str, error_node: NodeIndex) -> TypeId {
        if let Some(sym_id) = self.resolve_global_value_symbol(name) {
            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, error_node);
                return TypeId::ERROR;
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if (symbol.flags & symbol_flags::VALUE) == 0 {
                    self.error_type_only_value_at(name, error_node);
                    return TypeId::ERROR;
                }
            }
            return self.get_type_of_symbol(sym_id);
        }

        if self.is_known_global_value_name(name) {
            return TypeId::ANY; // Known global but unresolved - use ANY to allow property access
        }

        self.error_property_not_exist_at(name, TypeId::ANY, error_node);
        TypeId::ERROR
    }

    fn current_this_type(&self) -> Option<TypeId> {
        self.ctx.this_type_stack.last().copied()
    }

    fn is_constructor_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                !shape.construct_signatures.is_empty()
            }
            _ => false,
        }
    }

    fn is_class_symbol(&self, symbol_id: SymbolId) -> bool {
        use crate::binder::symbol_flags;
        if let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) {
            (symbol.flags & symbol_flags::CLASS) != 0
        } else {
            false
        }
    }

    /// Check if a node is a `super` expression.
    fn is_super_expression(&self, idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(node) = self.ctx.arena.get(idx) {
            node.kind == SyntaxKind::SuperKeyword as u16
        } else {
            false
        }
    }

    /// Get the type of a `super` keyword expression.
    ///
    /// When used in a constructor call (e.g., `super()`), this returns the
    /// base class constructor type. When used in property access (e.g., `super.method()`),
    /// the type is resolved through the normal property access mechanism.
    ///
    /// Returns the base class constructor type if in a derived class, otherwise ERROR.
    fn get_type_of_super_keyword(&mut self, idx: NodeIndex) -> TypeId {
        // Check if we're in a class context
        if let Some(ref class_info) = self.ctx.enclosing_class {
            // Get the base class
            if let Some(base_class_idx) = self.get_base_class_idx(class_info.class_idx) {
                // Get the base class node and class data
                if let Some(base_node) = self.ctx.arena.get(base_class_idx) {
                    if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                        // Return the constructor type of the base class
                        return self.get_class_constructor_type(base_class_idx, base_class);
                    }
                }
            }
        }
        // Not in a class or no base class - return ERROR
        TypeId::ERROR
    }

    /// Report an argument count mismatch error using solver diagnostics with source tracking.
    pub fn error_argument_count_mismatch_at(
        &mut self,
        expected: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.argument_count_mismatch(expected, got, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report "No overload matches this call" with related overload failures.
    pub fn error_no_overload_matches_at(
        &mut self,
        idx: NodeIndex,
        failures: &[crate::solver::PendingDiagnostic],
    ) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::{PendingDiagnostic, TypeFormatter};

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols);
        let mut related = Vec::new();
        let span =
            crate::solver::SourceSpan::new(self.ctx.file_name.as_str(), loc.start, loc.length());

        for failure in failures {
            let pending = PendingDiagnostic {
                span: Some(span.clone()),
                ..failure.clone()
            };
            let diag = formatter.render(&pending);
            if let Some(diag_span) = diag.span.as_ref() {
                related.push(DiagnosticRelatedInformation {
                    file: diag_span.file.to_string(),
                    start: diag_span.start,
                    length: diag_span.length,
                    message_text: diag.message.clone(),
                    category: DiagnosticCategory::Message,
                    code: diag.code,
                });
            }
        }

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::NO_OVERLOAD_MATCHES_CALL,
            category: DiagnosticCategory::Error,
            message_text: diagnostic_messages::NO_OVERLOAD_MATCHES.to_string(),
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: related,
        });
    }

    /// Report a "type is not callable" error using solver diagnostics with source tracking.
    pub fn error_not_callable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.not_callable(type_id, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Check if a type is a class constructor (typeof Class).
    /// Returns true for Callable types with only construct signatures (no call signatures).
    fn is_class_constructor_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        let Some(type_key) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        // A class constructor is a Callable with construct signatures but no call signatures
        if let TypeKey::Callable(shape_id) = type_key {
            let shape = self.ctx.types.callable_shape(shape_id);
            return !shape.construct_signatures.is_empty() && shape.call_signatures.is_empty();
        }

        false
    }

    /// Report TS2348: "Cannot invoke an expression whose type lacks a call signature"
    /// This is specifically for class constructors called without 'new'.
    pub fn error_class_constructor_without_new_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::solver::TypeFormatter;

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols);
        let type_str = formatter.format(type_id);

        let message = diagnostic_messages::CANNOT_INVOKE_EXPRESSION_LACKING_CALL_SIGNATURE
            .replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::CANNOT_INVOKE_EXPRESSION_WHOSE_TYPE_LACKS_CALL_SIGNATURE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report an excess property error using solver diagnostics with source tracking.
    pub fn error_excess_property_at(&mut self, prop_name: &str, target: TypeId, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.excess_property(prop_name, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a "Cannot assign to readonly property" error using solver diagnostics with source tracking.
    pub fn error_readonly_property_at(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::new(
                self.ctx.types,
                self.ctx.file_name.as_str(),
            );
            let diag = builder.readonly_property(prop_name, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS2803: Cannot assign to private method. Private methods are not writable.
    pub fn error_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(diagnostic_messages::CANNOT_ASSIGN_PRIVATE_METHOD, &[prop_name]);
            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_ASSIGN_TO_PRIVATE_METHOD,
            );
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Report TS2694: Namespace has no exported member.
    pub fn error_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Namespace '{}' has no exported member '{}'.",
                namespace_name, member_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: 2694,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2693: Symbol only refers to a type, but is used as a value.
    pub fn error_type_only_value_at(&mut self, name: &str, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2454: Variable is used before being assigned.
    pub fn error_variable_used_before_assigned_at(&mut self, name: &str, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::VARIABLE_USED_BEFORE_ASSIGNED, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::VARIABLE_USED_BEFORE_ASSIGNED,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2749: Symbol refers to a value, but is used as a type.
    pub fn error_value_only_type_at(&mut self, name: &str, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Create a diagnostic collector for batch error reporting.
    pub fn create_diagnostic_collector(&self) -> crate::solver::DiagnosticCollector<'_> {
        crate::solver::DiagnosticCollector::new(self.ctx.types, self.ctx.file_name.as_str())
    }

    /// Merge diagnostics from a collector into the checker's diagnostics.
    pub fn merge_diagnostics(&mut self, collector: &crate::solver::DiagnosticCollector) {
        for diag in collector.to_checker_diagnostics() {
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Format a type as a human-readable string using solver's TypeFormatter.
    pub fn format_type(&self, type_id: TypeId) -> String {
        let mut formatter =
            crate::solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols);
        formatter.format(type_id)
    }

    fn resolve_no_implicit_any_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@noimplicitany") {
            return value;
        }
        if let Some(strict) = Self::parse_test_option_bool(text, "@strict") {
            return strict;
        }
        self.ctx.no_implicit_any // Use the value from the strict flag
    }

    fn resolve_no_implicit_returns_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@noimplicitreturns") {
            return value;
        }
        // noImplicitReturns is NOT enabled by strict mode by default
        false
    }

    fn resolve_use_unknown_in_catch_variables_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@useunknownincatchvariables") {
            return value;
        }
        if let Some(strict) = Self::parse_test_option_bool(text, "@strict") {
            return strict;
        }
        self.ctx.use_unknown_in_catch_variables // Use the value from the strict flag
    }

    fn parse_test_option_bool(text: &str, key: &str) -> Option<bool> {
        for line in text.lines().take(32) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let is_comment =
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*');
            if !is_comment {
                break;
            }

            let lower = trimmed.to_ascii_lowercase();
            let Some(pos) = lower.find(key) else {
                continue;
            };
            let after_key = &lower[pos + key.len()..];
            let Some(colon_pos) = after_key.find(':') else {
                continue;
            };
            let value = after_key[colon_pos + 1..].trim();
            if value.starts_with("true") {
                return Some(true);
            }
            if value.starts_with("false") {
                return Some(false);
            }
        }
        None
    }

    // =========================================================================
    // Source File Checking (Full Traversal)
    // =========================================================================

    /// Check a source file and populate diagnostics.
    /// This is the entry point for type checking a parsed and bound file.
    pub fn check_source_file(&mut self, root_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };

        if let Some(sf) = self.ctx.arena.get_source_file(node) {
            self.ctx.no_implicit_any = self.resolve_no_implicit_any_from_source(&sf.text);
            self.ctx.no_implicit_returns = self.resolve_no_implicit_returns_from_source(&sf.text);
            self.ctx.use_unknown_in_catch_variables =
                self.resolve_use_unknown_in_catch_variables_from_source(&sf.text);

            // Type check each top-level statement
            for &stmt_idx in &sf.statements.nodes {
                self.check_statement(stmt_idx);
            }

            // Check for function overload implementations (2389, 2391)
            self.check_function_implementations(&sf.statements.nodes);

            // Check for export assignment with other exports (2309)
            self.check_export_assignment(&sf.statements.nodes);

            // Check for duplicate identifiers (2300)
            self.check_duplicate_identifiers();

            // Check for unused declarations (6133)
            // Only check for unused declarations when no_implicit_any is enabled (strict mode)
            // This prevents test files from reporting unused variable errors when they're testing specific behaviors
            if self.ctx.no_implicit_any {
                self.check_unused_declarations();
            }
        }
    }

    fn resolve_duplicate_decl_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CONSTRUCTOR => {
                    return Some(current);
                }
                _ => {}
            }

            let parent = self.ctx.arena.get_extended(current).map(|ext| ext.parent)?;
            if parent.is_none() {
                return None;
            }
            current = parent;
        }
        None
    }

    fn declaration_symbol_flags(&self, decl_idx: NodeIndex) -> Option<u32> {
        use crate::parser::node_flags;

        let decl_idx = self.resolve_duplicate_decl_node(decl_idx)?;
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let mut decl_flags = node.flags as u32;
                if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0 {
                    if let Some(parent) =
                        self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
                    {
                        if let Some(parent_node) = self.ctx.arena.get(parent) {
                            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                decl_flags |= parent_node.flags as u32;
                            }
                        }
                    }
                }
                if (decl_flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    Some(symbol_flags::BLOCK_SCOPED_VARIABLE)
                } else {
                    Some(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => Some(symbol_flags::FUNCTION),
            syntax_kind_ext::CLASS_DECLARATION => Some(symbol_flags::CLASS),
            syntax_kind_ext::INTERFACE_DECLARATION => Some(symbol_flags::INTERFACE),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => Some(symbol_flags::TYPE_ALIAS),
            syntax_kind_ext::ENUM_DECLARATION => Some(symbol_flags::REGULAR_ENUM),
            syntax_kind_ext::GET_ACCESSOR => Some(symbol_flags::GET_ACCESSOR),
            syntax_kind_ext::SET_ACCESSOR => Some(symbol_flags::SET_ACCESSOR),
            syntax_kind_ext::CONSTRUCTOR => Some(symbol_flags::CONSTRUCTOR),
            _ => None,
        }
    }

    fn excluded_symbol_flags(flags: u32) -> u32 {
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0 {
            return symbol_flags::FUNCTION_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0 {
            return symbol_flags::BLOCK_SCOPED_VARIABLE_EXCLUDES;
        }
        if (flags & symbol_flags::FUNCTION) != 0 {
            return symbol_flags::FUNCTION_EXCLUDES;
        }
        if (flags & symbol_flags::CLASS) != 0 {
            return symbol_flags::CLASS_EXCLUDES;
        }
        if (flags & symbol_flags::INTERFACE) != 0 {
            return symbol_flags::INTERFACE_EXCLUDES;
        }
        if (flags & symbol_flags::TYPE_ALIAS) != 0 {
            return symbol_flags::TYPE_ALIAS_EXCLUDES;
        }
        if (flags & symbol_flags::REGULAR_ENUM) != 0 {
            return symbol_flags::REGULAR_ENUM_EXCLUDES;
        }
        if (flags & symbol_flags::GET_ACCESSOR) != 0 {
            return symbol_flags::GET_ACCESSOR_EXCLUDES;
        }
        if (flags & symbol_flags::SET_ACCESSOR) != 0 {
            return symbol_flags::SET_ACCESSOR_EXCLUDES;
        }
        symbol_flags::NONE
    }

    fn declarations_conflict(flags_a: u32, flags_b: u32) -> bool {
        let excludes_a = Self::excluded_symbol_flags(flags_a);
        let excludes_b = Self::excluded_symbol_flags(flags_b);
        (flags_a & excludes_b) != 0 || (flags_b & excludes_a) != 0
    }

    /// Check for duplicate identifiers in the current scope set (TS2300).
    /// Uses persistent scopes when available, falling back to file_locals.
    fn check_duplicate_identifiers(&mut self) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let mut symbol_ids = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in &self.ctx.binder.scopes {
                for (_, &id) in scope.table.iter() {
                    symbol_ids.insert(id);
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                symbol_ids.insert(id);
            }
        }

        for sym_id in symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            if symbol.declarations.len() <= 1 {
                continue;
            }

            // Handle constructors separately - they use TS2392 (multiple constructor implementations), not TS2300
            if symbol.escaped_name == "constructor" {
                // Count only constructor implementations (with body), not overloads (without body)
                let implementations: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .filter_map(|&decl_idx| {
                        let node = self.ctx.arena.get(decl_idx)?;
                        let constructor = self.ctx.arena.get_constructor(node)?;
                        // Only count constructors with a body as implementations
                        if !constructor.body.is_none() {
                            Some(decl_idx)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Report TS2392 for multiple constructor implementations (not overloads)
                if implementations.len() > 1 {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages,
                    };
                    let message = diagnostic_messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS;
                    for &decl_idx in &implementations {
                        self.error_at_node(
                            decl_idx,
                            message,
                            diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS,
                        );
                    }
                }
                continue;
            }

            let mut declarations = Vec::new();
            for &decl_idx in &symbol.declarations {
                if let Some(flags) = self.declaration_symbol_flags(decl_idx) {
                    declarations.push((decl_idx, flags));
                }
            }

            if declarations.len() <= 1 {
                continue;
            }

            let mut conflicts = FxHashSet::default();
            for i in 0..declarations.len() {
                for j in (i + 1)..declarations.len() {
                    let (decl_idx, decl_flags) = declarations[i];
                    let (other_idx, other_flags) = declarations[j];
                    if Self::declarations_conflict(decl_flags, other_flags) {
                        conflicts.insert(decl_idx);
                        conflicts.insert(other_idx);
                    }
                }
            }

            if conflicts.is_empty() {
                continue;
            }

            // Check if we have any non-block-scoped declarations (var, function, etc.)
            // Imports (ALIAS) and let/const (BLOCK_SCOPED_VARIABLE) are block-scoped
            let has_non_block_scoped = declarations.iter().any(|(decl_idx, flags)| {
                conflicts.contains(decl_idx) && {
                    (flags & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::ALIAS)) == 0
                }
            });

            let name = symbol.escaped_name.clone();
            let (message, code) = if !has_non_block_scoped {
                // Pure block-scoped duplicates (let/const/import conflicts) emit TS2451
                (format_message(diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE, &[&name]), diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE)
            } else {
                // Mixed or non-block-scoped duplicates emit TS2300
                (format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]), diagnostic_codes::DUPLICATE_IDENTIFIER)
            };
            for (decl_idx, _) in declarations {
                if conflicts.contains(&decl_idx) {
                    let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    self.error_at_node(
                        error_node,
                        &message,
                        code,
                    );
                }
            }
        }
    }

    /// Get the name node of a declaration for error reporting.
    fn get_declaration_name_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.ctx.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.ctx.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                let class = self.ctx.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let interface = self.ctx.arena.get_interface(node)?;
                Some(interface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.ctx.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            _ => None,
        }
    }

    /// Check for unused declarations (TS6133).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    fn check_unused_declarations(&mut self) {
        // Temporarily disable unused declaration checking to focus on core functionality
        // The reference tracking system needs more work to avoid false positives
        // TODO: Re-enable and fix reference tracking system properly
        return;

        #[allow(unreachable_code)]
        {
        use crate::binder::symbol_flags;
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Collect all declared symbols
        let mut symbol_ids = FxHashSet::default();
        if !self.ctx.binder.scopes.is_empty() {
            for scope in &self.ctx.binder.scopes {
                for (_, &id) in scope.table.iter() {
                    symbol_ids.insert(id);
                }
            }
        } else {
            for (_, &id) in self.ctx.binder.file_locals.iter() {
                symbol_ids.insert(id);
            }
        }

        // Build a set of all referenced symbols
        let mut referenced_symbols = FxHashSet::default();
        for deps in self.ctx.symbol_dependencies.values() {
            for &sym_id in deps {
                referenced_symbols.insert(sym_id);
            }
        }

        // Check each symbol for usage
        for sym_id in symbol_ids {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            // Skip exported symbols - they're part of the public API
            if symbol.is_exported {
                continue;
            }

            // Skip special symbols like constructors, default exports, etc.
            if symbol.escaped_name == "constructor"
                || symbol.escaped_name == "default"
                || symbol.escaped_name == "__esModule"
            {
                continue;
            }

            // Skip imported symbols - they're tracked separately (UNUSED_IMPORT)
            if symbol.import_module.is_some() {
                continue;
            }

            // Skip symbols with underscore prefix - conventional "intentionally unused"
            if symbol.escaped_name.starts_with('_') {
                continue;
            }

            // Skip symbols that look like they might be used by external tools
            // Common patterns: test globals, build system variables, etc.
            let name_str = &symbol.escaped_name;
            if name_str.contains("Symbol") && name_str.contains("property") {
                // Skip Symbol.* property tests - these are often used dynamically
                continue;
            }

            // Skip symbols without declarations (shouldn't happen, but be safe)
            if symbol.declarations.is_empty() {
                continue;
            }

            // Check if this is a declaration type we want to check
            // We check: variables, functions, classes, enums, type aliases
            // We skip: interfaces, type parameters, namespaces (they're used structurally)
            let flags = symbol.flags;
            let is_checkable = (flags & symbol_flags::VARIABLE) != 0
                || (flags & symbol_flags::FUNCTION) != 0
                || (flags & symbol_flags::CLASS) != 0
                || (flags & symbol_flags::ENUM) != 0
                || (flags & symbol_flags::TYPE_ALIAS) != 0;

            // Skip certain types that are used structurally
            if (flags & symbol_flags::TYPE_PARAMETER) != 0
                || (flags & symbol_flags::INTERFACE) != 0
                || (flags & symbol_flags::SIGNATURE) != 0
            {
                continue;
            }

            if !is_checkable {
                continue;
            }

            // Check if this symbol is referenced anywhere
            if referenced_symbols.contains(&sym_id) {
                continue;
            }

            // Check if any declaration has an initializer or is ambient
            // Variables with initializers should not be reported as unused because
            // the act of declaration + initialization is meaningful usage.
            // Ambient declarations (declare) also should not be reported as unused.
            let should_skip = symbol.declarations.iter().any(|&decl_idx| {
                if let Some(node) = self.ctx.arena.get(decl_idx) {
                    if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                        // Skip if has initializer
                        if !var_decl.initializer.is_none() {
                            return true;
                        }
                    }
                    // Skip if ambient declaration (has DeclareKeyword modifier)
                    if (node.flags as u32) & crate::parser::node_flags::AMBIENT != 0 {
                        return true;
                    }
                }
                false
            });

            if should_skip {
                continue; // Don't report initialized or ambient variables as unused
            }

            // Symbol is not referenced - emit diagnostic for each declaration
            let name = symbol.escaped_name.clone();
            let message = format!("'{}' is declared but its value is never read.", name);

            for &decl_idx in &symbol.declarations {
                if let Some(name_node) = self.get_declaration_name_node(decl_idx) {
                    self.error_at_node(name_node, &message, diagnostic_codes::UNUSED_VARIABLE);
                }
            }
        }
        } // End unreachable code block
    }

    /// Check for duplicate parameter names in a parameter list (TS2300).
    fn check_duplicate_parameters(&mut self, parameters: &NodeList) {
        let mut seen_names = FxHashSet::default();
        for &param_idx in &parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            // Parameters can be identifiers or binding patterns
            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                self.collect_and_check_parameter_names(param.name, &mut seen_names);
            }
        }
    }

    /// Check for duplicate enum members (TS2300).
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(enum_node) = self.ctx.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_decl) = self.ctx.arena.get_enum(enum_node) else {
            return;
        };

        let mut seen_names = FxHashSet::default();
        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            // Get the member name
            let Some(name_node) = self.ctx.arena.get(member.name) else {
                continue;
            };
            let name_text = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                continue;
            };

            // Check for duplicate
            if seen_names.contains(&name_text) {
                let message =
                    format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name_text]);
                self.error_at_node(
                    member.name,
                    &message,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            } else {
                seen_names.insert(name_text);
            }
        }
    }

    /// Recursively collect names from identifiers or binding patterns and check for duplicates.
    fn collect_and_check_parameter_names(
        &mut self,
        name_idx: NodeIndex,
        seen: &mut FxHashSet<String>,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        let Some(node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        match node.kind {
            // Simple Identifier: parameter name
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = self.node_text(name_idx) {
                    let name_str = name.to_string();
                    if !seen.insert(name_str.clone()) {
                        self.error_at_node(
                            name_idx,
                            &format_message(
                                diagnostic_messages::DUPLICATE_IDENTIFIER,
                                &[&name_str],
                            ),
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }
            }
            // Object Binding Pattern: { a, b: c }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen);
                    }
                }
            }
            // Array Binding Pattern: [a, b]
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.ctx.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_and_check_binding_element(elem_idx, seen);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_and_check_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        seen: &mut FxHashSet<String>,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(elem_idx) else {
            return;
        };

        // Handle holes in array destructuring: [a, , b]
        if node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
            return;
        }

        if let Some(elem) = self.ctx.arena.get_binding_element(node) {
            // Recurse on the name (which can be an identifier or another pattern)
            self.collect_and_check_parameter_names(elem.name, seen);
        }
    }

    /// Check a statement and produce type errors.
    fn check_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::VARIABLE_STATEMENT => {
                self.check_variable_statement(stmt_idx);
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // ExpressionStatement stores expression index in data_index
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    // TS1359: Check for await expressions outside async function
                    self.check_await_expression(expr_stmt.expression);
                    // Then get the type for normal type checking
                    self.get_type_of_node(expr_stmt.expression);
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    // Check condition
                    self.check_await_expression(if_data.expression);
                    self.get_type_of_node(if_data.expression);
                    // Check then branch
                    self.check_statement(if_data.then_statement);
                    // Check else branch if present
                    if !if_data.else_statement.is_none() {
                        self.check_statement(if_data.else_statement);
                    }
                }
            }
            syntax_kind_ext::RETURN_STATEMENT => {
                self.check_return_statement(stmt_idx);
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    // Check for unreachable code before checking individual statements
                    self.check_unreachable_code_in_block(&block.statements.nodes);
                    for &inner_stmt in &block.statements.nodes {
                        self.check_statement(inner_stmt);
                    }
                    // Check for function overload implementations in blocks
                    self.check_function_implementations(&block.statements.nodes);
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.ctx.arena.get_function(node) {
                    let (_type_params, type_param_updates) =
                        self.push_type_parameters(&func.type_parameters);

                    // Check for parameter properties (error 2369)
                    // Parameter properties are only allowed in constructors
                    self.check_parameter_properties(&func.parameters.nodes);

                    // Check for duplicate parameter names (TS2300)
                    self.check_duplicate_parameters(&func.parameters);

                    // Check return type annotation for parameter properties in function types
                    if !func.type_annotation.is_none() {
                        self.check_type_for_parameter_properties(func.type_annotation);
                    }

                    // Check parameter type annotations for parameter properties
                    for &param_idx in &func.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                if !param.type_annotation.is_none() {
                                    self.check_type_for_parameter_properties(param.type_annotation);
                                }
                            }
                        }
                    }

                    for &param_idx in &func.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        self.maybe_report_implicit_any_parameter(param, false);
                    }

                    // Check function body if present
                    let has_type_annotation = !func.type_annotation.is_none();
                    if !func.body.is_none() {
                        let mut return_type = if has_type_annotation {
                            self.get_type_of_node(func.type_annotation)
                        } else {
                            // Use UNKNOWN to enforce strict checking
                            TypeId::UNKNOWN
                        };

                        self.cache_parameter_types(&func.parameters.nodes, None);

                        // Check that parameter default values are assignable to declared types (TS2322)
                        self.check_parameter_initializers(&func.parameters.nodes);

                        if !has_type_annotation {
                            return_type = self.infer_return_type_from_body(func.body, None);
                        }

                        // TS7010 (implicit any return) is emitted for functions without
                        // return type annotations when noImplicitAny is enabled and the return
                        // type cannot be inferred (e.g., is 'any' or only returns undefined)
                        // maybe_report_implicit_any_return handles the noImplicitAny check internally
                        let func_name = self.get_function_name_from_node(stmt_idx);
                        let name_node = if !func.name.is_none() {
                            Some(func.name)
                        } else {
                            None
                        };
                        self.maybe_report_implicit_any_return(
                            func_name,
                            name_node,
                            return_type,
                            has_type_annotation,
                            false,
                            stmt_idx,
                        );

                        // TS2705: Async function must return Promise
                        // Only check if there's an explicit return type annotation
                        // Note: Async generators (async function*) return AsyncGenerator, not Promise
                        if func.is_async
                            && !func.asterisk_token
                            && has_type_annotation
                            && !self.is_promise_type(return_type)
                        {
                            use crate::checker::types::diagnostics::{
                                diagnostic_codes, diagnostic_messages,
                            };
                            self.error_at_node(
                                func.type_annotation,
                                diagnostic_messages::ASYNC_FUNCTION_RETURNS_PROMISE,
                                diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                            );
                        }

                        // TS2705: Async function requires Promise constructor when Promise is not in lib
                        // This is a different check from the return type check above
                        // Check if function is async and Promise is not available in lib
                        if func.is_async && !func.asterisk_token && !self.ctx.has_promise_in_lib() {
                            use crate::checker::types::diagnostics::{
                                diagnostic_codes, diagnostic_messages,
                            };
                            self.error_at_node(
                                stmt_idx,
                                diagnostic_messages::ASYNC_FUNCTION_REQUIRES_PROMISE_CONSTRUCTOR,
                                diagnostic_codes::ASYNC_FUNCTION_RETURNS_PROMISE,
                            );
                        }

                        // Enter async context for await expression checking
                        if func.is_async {
                            self.ctx.enter_async_context();
                        }

                        self.push_return_type(return_type);
                        self.check_statement(func.body);

                        // Check for error 2355: function with return type must return a value
                        // Only check if there's an explicit return type annotation
                        let is_async = func.is_async;
                        let is_generator = func.asterisk_token;
                        let check_return_type = self.return_type_for_implicit_return_check(
                            return_type,
                            is_async,
                            is_generator,
                        );
                        let requires_return = self.requires_return_value(check_return_type);
                        let has_return = self.body_has_return_with_value(func.body);
                        let falls_through = self.function_body_falls_through(func.body);

                        // TS2355: Skip for async functions - they implicitly return Promise<void>
                        if has_type_annotation && requires_return && falls_through && !is_async {
                            if !has_return {
                                use crate::checker::types::diagnostics::diagnostic_codes;
                                self.error_at_node(
                                    func.type_annotation,
                                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                                    diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                                );
                            } else {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages,
                                };
                                self.error_at_node(
                                    func.type_annotation,
                                    diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                                );
                            }
                        } else if self.ctx.no_implicit_returns && has_return && falls_through {
                            // TS7030: noImplicitReturns - not all code paths return a value
                            use crate::checker::types::diagnostics::{
                                diagnostic_codes, diagnostic_messages,
                            };
                            let error_node = if !func.name.is_none() {
                                func.name
                            } else {
                                func.body
                            };
                            self.error_at_node(
                                error_node,
                                diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                                diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                            );
                        }

                        self.pop_return_type();

                        // Exit async context
                        if func.is_async {
                            self.ctx.exit_async_context();
                        }
                    } else if self.ctx.no_implicit_any && !has_type_annotation {
                        let is_ambient = self.has_declare_modifier(&func.modifiers)
                            || self.ctx.file_name.ends_with(".d.ts");
                        if is_ambient {
                            if let Some(func_name) = self.get_function_name_from_node(stmt_idx) {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::IMPLICIT_ANY_RETURN,
                                    &[&func_name, "any"],
                                );
                                let name_node = if !func.name.is_none() {
                                    Some(func.name)
                                } else {
                                    None
                                };
                                self.error_at_node(
                                    name_node.unwrap_or(stmt_idx),
                                    &message,
                                    diagnostic_codes::IMPLICIT_ANY_RETURN,
                                );
                            }
                        }
                    }

                    self.pop_type_parameters(type_param_updates);
                }
            }
            syntax_kind_ext::WHILE_STATEMENT | syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.get_type_of_node(loop_data.condition);
                    self.check_statement(loop_data.statement);
                }
            }
            syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    if !loop_data.initializer.is_none() {
                        // Check if initializer is a variable declaration list
                        if let Some(init_node) = self.ctx.arena.get(loop_data.initializer) {
                            if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                self.check_variable_declaration_list(loop_data.initializer);
                            } else {
                                self.get_type_of_node(loop_data.initializer);
                            }
                        }
                    }
                    if !loop_data.condition.is_none() {
                        self.get_type_of_node(loop_data.condition);
                    }
                    if !loop_data.incrementor.is_none() {
                        self.get_type_of_node(loop_data.incrementor);
                    }
                    self.check_statement(loop_data.statement);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_data) = self.ctx.arena.get_for_in_of(node) {
                    // Check if initializer is a variable declaration
                    if let Some(init_node) = self.ctx.arena.get(for_data.initializer) {
                        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                            self.check_variable_declaration_list(for_data.initializer);
                        } else {
                            self.get_type_of_node(for_data.initializer);
                        }
                    }
                    self.get_type_of_node(for_data.expression);
                    self.check_statement(for_data.statement);
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.check_statement(try_data.try_block);
                    if !try_data.catch_clause.is_none() {
                        if let Some(catch_node) = self.ctx.arena.get(try_data.catch_clause) {
                            if let Some(catch) = self.ctx.arena.get_catch_clause(catch_node) {
                                if !catch.variable_declaration.is_none() {
                                    self.check_variable_declaration(catch.variable_declaration);
                                }
                                self.check_statement(catch.block);
                            }
                        }
                    }
                    if !try_data.finally_block.is_none() {
                        self.check_statement(try_data.finally_block);
                    }
                }
            }
            // Interface declarations need parameter property checks
            syntax_kind_ext::INTERFACE_DECLARATION => {
                self.check_interface_declaration(stmt_idx);
            }
            // Export declarations - descend into the wrapped declaration
            syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_decl) = self.ctx.arena.get_export_decl(node) {
                    // Check module specifier for unresolved modules (TS2792)
                    // This handles cases like: export * as ns from './nonexistent';
                    if !export_decl.module_specifier.is_none() {
                        self.check_export_module_specifier(stmt_idx);
                    }
                    // Check the wrapped declaration (function, class, variable, etc.)
                    if !export_decl.export_clause.is_none() {
                        self.check_statement(export_decl.export_clause);
                    }
                }
            }
            // Type alias declarations - check the type for accessor body and parameter property errors
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                    let (_params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                    // Check the type for accessor bodies in ambient context and parameter properties
                    self.check_type_for_missing_names(type_alias.type_node);
                    self.check_type_for_parameter_properties(type_alias.type_node);
                    self.pop_type_parameters(updates);
                }
            }
            // Enum declarations - check for duplicate enum members (TS2300)
            syntax_kind_ext::ENUM_DECLARATION => {
                self.check_enum_duplicate_members(stmt_idx);
            }
            // Other type declarations that don't need action here
            syntax_kind_ext::EMPTY_STATEMENT
            | syntax_kind_ext::DEBUGGER_STATEMENT
            | syntax_kind_ext::BREAK_STATEMENT
            | syntax_kind_ext::CONTINUE_STATEMENT => {
                // No action needed
            }
            syntax_kind_ext::IMPORT_DECLARATION => {
                self.check_import_declaration(stmt_idx);
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                // Check module declaration (errors 5061, 2819, etc.)
                let mut checker =
                    crate::checker::declarations::DeclarationChecker::new(&mut self.ctx);
                checker.check_module_declaration(stmt_idx);

                // Check module body for function overload implementations
                if let Some(module) = self.ctx.arena.get_module(node) {
                    if !module.body.is_none() {
                        self.check_module_body(module.body);
                    }
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                self.check_class_declaration(stmt_idx);
            }
            _ => {
                // Catch-all for other statement types
                self.get_type_of_node(stmt_idx);
            }
        }
    }

    /// Check a variable statement (var/let/const declarations).
    fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(var) = self.ctx.arena.get_variable(node) {
            // VariableStatement.declarations contains VariableDeclarationList nodes
            for &list_idx in &var.declarations.nodes {
                self.check_variable_declaration_list(list_idx);
            }
        }
    }

    /// Check a variable declaration list (var/let/const x, y, z).
    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };

        // VariableDeclarationList uses the same VariableData structure
        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            // Now these are actual VariableDeclaration nodes
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration(decl_idx);
            }
        }
    }

    /// Check a single variable declaration.
    fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // Get the variable name for adding to local scope
        let var_name = if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                Some(ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            None
        };

        let is_catch_variable = self.is_catch_clause_variable_declaration(decl_idx);

        let compute_final_type = |checker: &mut ThinCheckerState| -> TypeId {
            let mut has_type_annotation = !var_decl.type_annotation.is_none();
            let mut declared_type = if has_type_annotation {
                checker.get_type_from_type_node(var_decl.type_annotation)
            } else if is_catch_variable && checker.ctx.use_unknown_in_catch_variables {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };
            if !has_type_annotation {
                if let Some(jsdoc_type) = checker.jsdoc_type_annotation_for_node(decl_idx) {
                    declared_type = jsdoc_type;
                    has_type_annotation = true;
                }
            }

            // If there's a type annotation, that determines the type (even for 'any')
            if has_type_annotation {
                if !var_decl.initializer.is_none() {
                    // Set contextual type for the initializer (but not for 'any')
                    let prev_context = checker.ctx.contextual_type;
                    if declared_type != TypeId::ANY {
                        checker.ctx.contextual_type = Some(declared_type);
                    }
                    let init_type = checker.get_type_of_node(var_decl.initializer);
                    checker.ctx.contextual_type = prev_context;

                    // Check assignability (skip for 'any' since anything is assignable to any)
                    if declared_type != TypeId::ANY {
                        if let Some((source_level, target_level)) =
                            checker.constructor_accessibility_mismatch_for_var_decl(var_decl)
                        {
                            checker.error_constructor_accessibility_not_assignable(
                                init_type,
                                declared_type,
                                source_level,
                                target_level,
                                var_decl.initializer,
                            );
                        } else if !checker.is_assignable_to(init_type, declared_type) {
                            if !checker.should_skip_weak_union_error(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                            ) {
                                checker.error_type_not_assignable_with_reason_at(
                                    init_type,
                                    declared_type,
                                    var_decl.initializer,
                                );
                            }
                        }

                        // For object literals, also check for excess properties
                        if let Some(init_node) = checker.ctx.arena.get(var_decl.initializer) {
                            if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                                checker.check_object_literal_excess_properties(
                                    init_type,
                                    declared_type,
                                    var_decl.initializer,
                                );
                            }
                        }
                    }
                }
                // Type annotation determines the final type
                return declared_type;
            }

            // No type annotation - infer from initializer
            if !var_decl.initializer.is_none() {
                let init_type = checker.get_type_of_node(var_decl.initializer);
                if let Some(literal_type) =
                    checker.literal_type_from_initializer(var_decl.initializer)
                {
                    if checker.is_const_variable_declaration(decl_idx) {
                        return literal_type;
                    }
                    return checker.widen_literal_type(literal_type);
                }
                init_type
            } else {
                declared_type
            }
        };

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
            self.push_symbol_dependency(sym_id, true);
            let final_type = compute_final_type(self);
            self.pop_symbol_dependency();

            // TS7005: Variable implicitly has an 'any' type
            // Report this error when noImplicitAny is enabled and the variable has no type annotation
            // and the inferred type is 'any'
            // Skip destructuring patterns - TypeScript doesn't emit TS7005 for them
            // because binding elements with default values can infer their types
            if self.ctx.no_implicit_any
                && var_decl.type_annotation.is_none()
                && final_type == TypeId::ANY
            {
                // Check if the variable name is a destructuring pattern
                let is_destructuring_pattern = self.ctx.arena.get(var_decl.name)
                    .map_or(false, |name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                if !is_destructuring_pattern {
                    if let Some(ref name) = var_name {
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::VARIABLE_IMPLICIT_ANY,
                            &[name, "any"],
                        );
                        self.error_at_node(
                            var_decl.name,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY,
                        );
                    }
                }
            }

            // Check for variable redeclaration in the current scope (TS2403).
            // Note: This applies specifically to 'var' merging where types must match.
            // let/const duplicates are caught earlier by the binder (TS2451).
            // Skip TS2403 for mergeable declarations (namespace, enum, class, interface, function overloads).
            if let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied() {
                // Check if this is a mergeable declaration by looking at the node kind.
                // Mergeable declarations: namespace/module, enum, class, interface, function.
                // When these are declared with the same name, they merge instead of conflicting.
                let is_mergeable_declaration = if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                {
                    matches!(
                        decl_node.kind,
                        syntax_kind_ext::MODULE_DECLARATION  // namespace/module
                            | syntax_kind_ext::ENUM_DECLARATION // enum
                            | syntax_kind_ext::CLASS_DECLARATION // class
                            | syntax_kind_ext::INTERFACE_DECLARATION // interface
                            | syntax_kind_ext::FUNCTION_DECLARATION // function
                    )
                } else {
                    false
                };

                if !is_mergeable_declaration
                    && !self.are_var_decl_types_compatible(prev_type, final_type)
                {
                    if let Some(ref name) = var_name {
                        self.error_subsequent_variable_declaration(
                            name, prev_type, final_type, decl_idx,
                        );
                    }
                } else {
                    let refined = self.refine_var_decl_type(prev_type, final_type);
                    if refined != prev_type {
                        self.ctx.var_decl_types.insert(sym_id, refined);
                    }
                }
            } else {
                self.ctx.var_decl_types.insert(sym_id, final_type);
            }

            if !self.ctx.symbol_types.contains_key(&sym_id) {
                self.cache_symbol_type(sym_id, final_type);
            }
        } else {
            compute_final_type(self);
        }

        // If the variable name is a binding pattern, check binding element default values
        if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                let pattern_type = if !var_decl.type_annotation.is_none() {
                    self.get_type_from_type_node(var_decl.type_annotation)
                } else if is_catch_variable && self.ctx.use_unknown_in_catch_variables {
                    TypeId::UNKNOWN
                } else {
                    TypeId::ANY
                };
                self.check_binding_pattern(var_decl.name, pattern_type);
            }
        }
    }

    /// Check binding pattern elements and their default values for type correctness.
    ///
    /// This function traverses a binding pattern (object or array destructuring) and verifies
    /// that any default values provided in binding elements are assignable to their expected types.
    fn check_binding_pattern(&mut self, pattern_idx: NodeIndex, pattern_type: TypeId) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Traverse binding elements
        for &element_idx in &pattern_data.elements.nodes {
            self.check_binding_element(element_idx, pattern_type);
        }
    }

    /// Check a single binding element for default value assignability.
    fn check_binding_element(&mut self, element_idx: NodeIndex, parent_type: TypeId) {
        let Some(element_node) = self.ctx.arena.get(element_idx) else {
            return;
        };

        let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
            return;
        };

        // Get the expected type for this binding element from the parent type
        let element_type = if parent_type != TypeId::ANY {
            // For object binding patterns, look up the property type
            // For array binding patterns, look up the tuple element type
            // For now, we'll use a simplified approach
            self.get_binding_element_type(element_idx, parent_type, element_data)
        } else {
            TypeId::ANY
        };

        // Check if there's a default value (initializer)
        if !element_data.initializer.is_none() {
            if element_type != TypeId::ANY {
                let default_value_type = self.get_type_of_node(element_data.initializer);

                if !self.is_assignable_to(default_value_type, element_type) {
                    self.error_type_not_assignable_with_reason_at(
                        default_value_type,
                        element_type,
                        element_data.initializer,
                    );
                }
            }
        }

        // If the name is a nested binding pattern, recursively check it
        if let Some(name_node) = self.ctx.arena.get(element_data.name) {
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                self.check_binding_pattern(element_data.name, element_type);
            }
        }
    }

    /// Get the expected type for a binding element from its parent type.
    fn get_binding_element_type(
        &mut self,
        element_idx: NodeIndex,
        parent_type: TypeId,
        element_data: &crate::parser::thin_node::BindingElementData,
    ) -> TypeId {
        use crate::solver::TypeKey;

        // Get the property name or index
        let property_name = if !element_data.property_name.is_none() {
            // { x: a } - property_name is "x"
            if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                if let Some(ident) = self.ctx.arena.get_identifier(prop_node) {
                    Some(ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            // { x } - the name itself is the property name
            if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    Some(ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if parent_type == TypeId::UNKNOWN {
            if let Some(prop_name_str) = property_name.as_deref() {
                let error_node = if !element_data.property_name.is_none() {
                    element_data.property_name
                } else if !element_data.name.is_none() {
                    element_data.name
                } else {
                    element_idx
                };
                self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
            }
            return TypeId::UNKNOWN;
        }

        if let Some(prop_name_str) = property_name {
            // Look up the property type in the parent type
            match self.ctx.types.lookup(parent_type) {
                Some(TypeKey::Object(shape_id)) => {
                    let shape = self.ctx.types.object_shape(shape_id);
                    // Find the property by comparing names
                    for prop in shape.properties.as_slice() {
                        if self.ctx.types.resolve_atom_ref(prop.name).as_ref() == prop_name_str {
                            return prop.type_id;
                        }
                    }
                    TypeId::ANY
                }
                _ => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }

    /// Check object literal assignment for excess properties.
    ///
    /// **Note**: This check is specific to object literals and is NOT part of general
    /// structural subtyping. Excess properties in object literals are errors, but
    /// when assigning from a variable with extra properties, it's allowed.
    /// See https://github.com/microsoft/TypeScript/issues/13813,
    /// https://github.com/microsoft/TypeScript/issues/18075,
    /// https://github.com/microsoft/TypeScript/issues/28616.
    ///
    /// Missing property errors are handled by the solver's `explain_failure` API
    /// via `error_type_not_assignable_with_reason_at`, so we only check excess
    /// properties here to avoid duplication.
    fn check_object_literal_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use crate::solver::TypeKey;

        // Get the properties of both types
        let source_shape = match self.ctx.types.lookup(source) {
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                self.ctx.types.object_shape(shape_id)
            }
            _ => return,
        };

        let source_props = source_shape.properties.as_slice();
        let resolved_target = self.resolve_type_for_property_access(target);

        match self.ctx.types.lookup(resolved_target) {
            Some(TypeKey::Object(shape_id)) => {
                let target_shape = self.ctx.types.object_shape(shape_id);
                let target_props = target_shape.properties.as_slice();

                // Empty object {} accepts any properties - no excess property check needed.
                // This is a key TypeScript behavior: {} means "any non-nullish value".
                // See https://github.com/microsoft/TypeScript/issues/60582
                if target_props.is_empty() {
                    return;
                }

                // Check for excess properties in source that don't exist in target
                // This is the "freshness" or "strict object literal" check
                for source_prop in source_props {
                    let exists_in_target = target_props.iter().any(|p| p.name == source_prop.name);
                    if !exists_in_target {
                        let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                        self.error_excess_property_at(&prop_name, target, idx);
                    }
                }
            }
            Some(TypeKey::Union(members_id)) => {
                let members = self.ctx.types.type_list(members_id);
                let mut target_shapes = Vec::new();

                for &member in members.iter() {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let shape = match self.ctx.types.lookup(resolved_member) {
                        Some(TypeKey::Object(shape_id))
                        | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                            self.ctx.types.object_shape(shape_id)
                        }
                        _ => continue,
                    };

                    if shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                    {
                        return;
                    }

                    target_shapes.push(shape);
                }

                if target_shapes.is_empty() {
                    return;
                }

                for source_prop in source_props {
                    let exists_in_target = target_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    });
                    if !exists_in_target {
                        let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                        self.error_excess_property_at(&prop_name, target, idx);
                    }
                }
            }
            _ => return,
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    /// Check if an assignment target is a readonly property.
    /// Reports error TS2540 if trying to assign to a readonly property.
    fn check_readonly_assignment(&mut self, target_idx: NodeIndex, expr_idx: NodeIndex) {
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match target_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {}
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(target_node) {
                    let object_type = self.get_type_of_node(access.expression);
                    if object_type == TypeId::ANY
                        || object_type == TypeId::UNKNOWN
                        || object_type == TypeId::ERROR
                    {
                        return;
                    }

                    let index_type = self.get_type_of_node(access.name_or_argument);
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        self.error_readonly_property_at(&name, target_idx);
                    }
                }
                return;
            }
            _ => return,
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return;
        };

        // Check if this is a private identifier (method or field)
        // Private methods are always readonly
        if self.is_private_identifier_name(access.name_or_argument) {
            let prop_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                return;
            };

            // Check if this private identifier is a method (not a field)
            // by resolving the symbol and checking if any declaration is a method
            let (symbols, _) = self.resolve_private_identifier_symbols(access.name_or_argument);
            if !symbols.is_empty() {
                let is_method = symbols.iter().any(|&sym_id| {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        symbol.declarations.iter().any(|&decl_idx| {
                            if let Some(node) = self.ctx.arena.get(decl_idx) {
                                return node.kind == syntax_kind_ext::METHOD_DECLARATION;
                            }
                            false
                        })
                    } else {
                        false
                    }
                });

                if is_method {
                    self.error_private_method_not_writable(&prop_name, target_idx);
                    return;
                }
            }
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };

        let prop_name = ident.escaped_text.clone();

        // Get the type of the object being accessed
        let obj_type = self.get_type_of_node(access.expression);

        // Check if the property is readonly in the object type (solver types)
        if self.is_property_readonly(obj_type, &prop_name) {
            self.error_readonly_property_at(&prop_name, target_idx);
            return;
        }

        // Also check AST-level readonly on class properties
        // Get the class name from the object expression (for `c.ro`, get the type of `c`)
        if let Some(class_name) = self.get_class_name_from_expression(access.expression) {
            if self.is_class_property_readonly(&class_name, &prop_name) {
                self.error_readonly_property_at(&prop_name, target_idx);
            }
        }
    }

    /// Get the class name from an expression, if it's a class instance.
    fn get_class_name_from_expression(&mut self, expr_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        // If it's a simple identifier, look up its type from the binder
        if self.ctx.arena.get_identifier(node).is_some() {
            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
                let type_id = self.get_type_of_symbol(sym_id);
                if let Some(class_name) = self.get_class_name_from_type(type_id) {
                    return Some(class_name);
                }
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    // Get the value declaration and check if it's a variable with new Class()
                    if !symbol.value_declaration.is_none() {
                        return self.get_class_name_from_var_decl(symbol.value_declaration);
                    }
                }
            }
        }

        None
    }

    /// Get the class name from a variable declaration that initializes to `new ClassName()`.
    fn get_class_name_from_var_decl(&self, decl_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return None;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return None;
        };

        if var_decl.initializer.is_none() {
            return None;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return None;
        };

        // Check if initializer is `new ClassName()`
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        // Call and new expressions share CallExprData
        let Some(new_expr) = self.ctx.arena.get_call_expr(init_node) else {
            return None;
        };

        // Get the class name from the new expression
        let Some(expr_node) = self.ctx.arena.get(new_expr.expression) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            return Some(ident.escaped_text.clone());
        }

        None
    }

    fn get_class_declaration_from_symbol(&self, sym_id: SymbolId) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.value_declaration.is_none() {
            let decl_idx = symbol.value_declaration;
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if self.ctx.arena.get_class(node).is_some() {
                    return Some(decl_idx);
                }
            }
        }

        for &decl_idx in &symbol.declarations {
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if self.ctx.arena.get_class(node).is_some() {
                    return Some(decl_idx);
                }
            }
        }

        None
    }

    fn get_class_name_from_decl(&self, class_idx: NodeIndex) -> String {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return "<anonymous>".to_string();
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return "<anonymous>".to_string();
        };

        if !class.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    return ident.escaped_text.clone();
                }
            }
        }

        "<anonymous>".to_string()
    }

    fn get_base_class_idx(&self, class_idx: NodeIndex) -> Option<NodeIndex> {
        use crate::scanner::SyntaxKind;

        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            let base_sym_id = self.resolve_heritage_symbol(expr_idx)?;
            return self.get_class_declaration_from_symbol(base_sym_id);
        }

        None
    }

    fn is_class_derived_from(&self, derived_idx: NodeIndex, base_idx: NodeIndex) -> bool {
        use rustc_hash::FxHashSet;

        if derived_idx == base_idx {
            return true;
        }

        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();
        let mut current = derived_idx;

        while visited.insert(current) {
            let Some(parent) = self.get_base_class_idx(current) else {
                return false;
            };
            if parent == base_idx {
                return true;
            }
            current = parent;
        }

        false
    }

    fn get_class_decl_from_type(&self, type_id: TypeId) -> Option<NodeIndex> {
        use crate::solver::TypeKey;

        fn parse_brand_name(name: &str) -> Option<Result<SymbolId, NodeIndex>> {
            const NODE_PREFIX: &str = "__private_brand_node_";
            const PREFIX: &str = "__private_brand_";

            if let Some(rest) = name.strip_prefix(NODE_PREFIX) {
                let node_id: u32 = rest.parse().ok()?;
                return Some(Err(NodeIndex(node_id)));
            }
            if let Some(rest) = name.strip_prefix(PREFIX) {
                let sym_id: u32 = rest.parse().ok()?;
                return Some(Ok(SymbolId(sym_id)));
            }

            None
        }

        fn collect_candidates<'a>(
            checker: &ThinCheckerState<'a>,
            type_id: TypeId,
            out: &mut Vec<NodeIndex>,
        ) {
            let Some(key) = checker.ctx.types.lookup(type_id) else {
                return;
            };

            match key {
                TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                    let shape = checker.ctx.types.object_shape(shape_id);
                    for prop in &shape.properties {
                        let name = checker.ctx.types.resolve_atom_ref(prop.name);
                        if let Some(parsed) = parse_brand_name(&name) {
                            let class_idx = match parsed {
                                Ok(sym_id) => checker.get_class_declaration_from_symbol(sym_id),
                                Err(node_idx) => Some(node_idx),
                            };
                            if let Some(class_idx) = class_idx {
                                out.push(class_idx);
                            }
                        }
                    }
                }
                TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                    let list = checker.ctx.types.type_list(list_id);
                    for &member in list.iter() {
                        collect_candidates(checker, member, out);
                    }
                }
                _ => {}
            }
        }

        let mut candidates = Vec::new();
        collect_candidates(self, type_id, &mut candidates);
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }

        for &candidate in &candidates {
            if candidates
                .iter()
                .all(|&other| candidate == other || self.is_class_derived_from(candidate, other))
            {
                return Some(candidate);
            }
        }

        None
    }

    /// Get the class name from a TypeId if it represents a class instance.
    fn get_class_name_from_type(&self, type_id: TypeId) -> Option<String> {
        self.get_class_decl_from_type(type_id)
            .map(|class_idx| self.get_class_name_from_decl(class_idx))
    }

    /// Check if a property is readonly in a class declaration (by looking at AST).
    fn is_class_property_readonly(&self, class_name: &str, prop_name: &str) -> bool {
        use crate::scanner::SyntaxKind;

        // Find the class declaration by name
        if let Some(sym_id) = self.ctx.binder.file_locals.get(class_name) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let decl_idx = if !symbol.value_declaration.is_none() {
                    symbol.value_declaration
                } else if let Some(&idx) = symbol.declarations.first() {
                    idx
                } else {
                    return false;
                };

                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    return false;
                };

                let Some(class) = self.ctx.arena.get_class(node) else {
                    return false;
                };

                // Find the property in the class members
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };

                    if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                        if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                            // Get the property name
                            if let Some(pname) = self.get_property_name(prop.name) {
                                if pname == prop_name {
                                    // Check if this property has readonly modifier
                                    return self.has_readonly_modifier(&prop.modifiers);
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check if modifiers include the 'readonly' keyword.
    fn has_readonly_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ReadonlyKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if modifiers include a parameter property keyword.
    fn has_parameter_property_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::PublicKeyword as u16
                        || mod_node.kind == SyntaxKind::PrivateKeyword as u16
                        || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                        || mod_node.kind == SyntaxKind::ReadonlyKeyword as u16
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a property is marked readonly in a type.
    fn is_property_readonly(&self, type_id: TypeId, prop_name: &str) -> bool {
        use crate::solver::QueryDatabase;

        self.ctx.types.is_property_readonly(type_id, prop_name)
    }

    fn is_readonly_index_signature(
        &self,
        type_id: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        use crate::solver::QueryDatabase;

        self.ctx
            .types
            .is_readonly_index_signature(type_id, wants_string, wants_number)
    }

    fn get_readonly_element_access_name(
        &mut self,
        object_type: TypeId,
        index_expr: NodeIndex,
        index_type: TypeId,
    ) -> Option<String> {
        if let Some(name) = self.get_literal_string_from_node(index_expr) {
            if self.is_property_readonly(object_type, &name) {
                return Some(name);
            }
            return None;
        }

        if let Some(index) = self.get_literal_index_from_node(index_expr) {
            let name = index.to_string();
            if self.is_property_readonly(object_type, &name) {
                return Some(name);
            }
            return None;
        }

        if let Some((string_keys, number_keys)) = self.get_literal_key_union_from_type(index_type) {
            for key in string_keys {
                let name = self.ctx.types.resolve_atom(key);
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }

            for key in number_keys {
                let name = format!("{}", key);
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }
            return None;
        }

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(index_type) {
            if self.is_readonly_index_signature(object_type, wants_string, wants_number) {
                return Some("index signature".to_string());
            }
        }

        None
    }

    /// Check a return statement.
    fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(return_data) = self.ctx.arena.get_return_statement(node) else {
            return;
        };

        // Get the expected return type from the function context
        let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);

        // Get the type of the return expression (if any)
        let return_type = if !return_data.expression.is_none() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);

            let prev_context = self.ctx.contextual_type;
            if expected_type != TypeId::ANY && !self.type_contains_error(expected_type) {
                self.ctx.contextual_type = Some(expected_type);
            }
            let return_type = self.get_type_of_node(return_data.expression);
            self.ctx.contextual_type = prev_context;
            return_type
        } else {
            // `return;` without expression returns undefined
            TypeId::UNDEFINED
        };

        // Ensure all Application type symbols are resolved before assignability check
        self.ensure_application_symbols_resolved(return_type);
        self.ensure_application_symbols_resolved(expected_type);

        // Check if the return type is assignable to the expected type
        // Exception: Constructors allow `return;` without an expression (no assignability check)
        let is_constructor_return_without_expr = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_constructor)
            .unwrap_or(false)
            && return_data.expression.is_none();

        if expected_type != TypeId::ANY
            && !is_constructor_return_without_expr
            && !self.is_assignable_to(return_type, expected_type)
        {
            // Report error at the return expression (or at return keyword if no expression)
            let error_node = if !return_data.expression.is_none() {
                return_data.expression
            } else {
                stmt_idx
            };
            if !self.should_skip_weak_union_error(return_type, expected_type, error_node) {
                self.error_type_not_assignable_with_reason_at(
                    return_type,
                    expected_type,
                    error_node,
                );
            }
        }

        if expected_type != TypeId::ANY
            && expected_type != TypeId::UNKNOWN
            && !return_data.expression.is_none()
        {
            if let Some(expr_node) = self.ctx.arena.get(return_data.expression) {
                if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.check_object_literal_excess_properties(
                        return_type,
                        expected_type,
                        return_data.expression,
                    );
                }
            }
        }
    }

    /// Check an import declaration for unresolved modules and missing exports.
    /// Emits TS2792 when the module cannot be resolved.
    /// Emits TS2305 when a module exists but doesn't export a specific member.
    fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(import.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules {
            if resolved.contains(module_name) {
                // Module exists, check if individual imports are exported
                self.check_imported_members(import, module_name);
                return;
            }
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        // This enables resolving imports from other files in the same compilation
        if self.ctx.binder.module_exports.contains_key(module_name) {
            // Module exists, check if individual imports are exported
            self.check_imported_members(import, module_name);
            return;
        }

        // Note: We do NOT skip TS2792 for declared_modules (ambient modules).
        // Imports from ambient modules should emit TS2792 because ambient modules
        // don't provide runtime values - they only provide type information.
        // If you want to use an ambient module's types, you should use `import type`
        // or reference the types directly in a type annotation.

        // In single-file mode, any external import is considered unresolved.
        // This is correct because WASM checker operates on individual files
        // without access to the module graph (aside from ambient module declarations).
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(import.module_specifier, &message, diagnostic_codes::CANNOT_FIND_MODULE);
    }

    /// Check if individual imported members exist in the module's exports.
    /// Emits TS2305 for each missing export.
    fn check_imported_members(&mut self, import: &ImportDeclData, module_name: &str) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        // Get the import clause
        let clause_node = match self.ctx.arena.get(import.import_clause) {
            Some(node) => node,
            None => return,
        };

        let clause = match self.ctx.arena.get_import_clause(clause_node) {
            Some(c) => c,
            None => return,
        };

        // Get named_bindings (NamedImports or NamespaceImport)
        let bindings_node = match self.ctx.arena.get(clause.named_bindings) {
            Some(node) => node,
            None => return,
        };

        // Check if this is NamedImports (import { a, b })
        if bindings_node.kind == crate::parser::syntax_kind_ext::NAMED_IMPORTS {
            let named_imports = match self.ctx.arena.get_named_imports(bindings_node) {
                Some(ni) => ni,
                None => return,
            };

            // Get the module's exports table
            let exports_table = match self.ctx.binder.module_exports.get(module_name) {
                Some(table) => table,
                None => return,
            };

            // Check each import specifier
            for element_idx in &named_imports.elements.nodes {
                let element_node = match self.ctx.arena.get(*element_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let specifier = match self.ctx.arena.get_specifier(element_node) {
                    Some(s) => s,
                    None => continue,
                };

                // Get the name being imported (property_name if present, otherwise name)
                let name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };

                let name_node = match self.ctx.arena.get(name_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let identifier = match self.ctx.arena.get_identifier(name_node) {
                    Some(id) => id,
                    None => continue,
                };

                let import_name = &identifier.escaped_text;

                // Check if this import exists in the module's exports
                if !exports_table.has(import_name) {
                    // Emit TS2305: Module has no exported member
                    let message = format_message(
                        diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                        &[module_name, import_name]
                    );
                    self.error_at_node(specifier.name, &message, diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER);
                }
            }
        }
        // Note: Namespace imports (import * as ns) don't need individual checks
        // Default imports don't need checks here (they're handled differently)
    }

    /// Check an export declaration's module specifier for unresolved modules.
    /// Emits TS2792 when the module cannot be resolved.
    /// Handles cases like: export * as ns from './nonexistent';
    fn check_export_module_specifier(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
            return;
        };

        // Get module specifier string
        let Some(spec_node) = self.ctx.arena.get(export_decl.module_specifier) else {
            return;
        };

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules {
            if resolved.contains(module_name) {
                return;
            }
        }

        // Emit TS2792 for unresolved export module specifiers
        let message = format_message(diagnostic_messages::CANNOT_FIND_MODULE, &[module_name]);
        self.error_at_node(export_decl.module_specifier, &message, diagnostic_codes::CANNOT_FIND_MODULE);
    }

    /// Check heritage clauses (extends/implements) for unresolved names.
    /// Emits TS2304 when a referenced name cannot be resolved.
    fn check_heritage_clauses_for_unresolved_names(
        &mut self,
        heritage_clauses: &Option<crate::parser::NodeList>,
    ) {
        use crate::parser::syntax_kind_ext::HERITAGE_CLAUSE;
        use crate::scanner::SyntaxKind;

        let Some(clauses) = heritage_clauses else {
            return;
        };

        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            if clause_node.kind != HERITAGE_CLAUSE {
                continue;
            }

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Check if this is an extends clause (for TS2507 errors)
            let is_extends_clause = heritage.token == SyntaxKind::ExtendsKeyword as u16;

            // Check each type in the heritage clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression (identifier or property access) from ExpressionWithTypeArguments
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Try to resolve the heritage symbol
                if let Some(heritage_sym) = self.resolve_heritage_symbol(expr_idx) {
                    // Symbol was resolved - check if it represents a constructor type for extends clauses
                    if is_extends_clause {
                        let symbol_type = self.get_type_of_symbol(heritage_sym);
                        if !self.is_constructor_type(symbol_type) && !self.is_class_symbol(heritage_sym) {
                            // Resolved to a non-constructor type - emit TS2507
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                );
                            }
                        }
                    }
                } else {
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                        // Check for literals - emit TS2507 for extends clauses
                        let literal_type_name: Option<&str> = match expr_node.kind {
                            k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                            k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                            k if k == SyntaxKind::TrueKeyword as u16 => Some("true"),
                            k if k == SyntaxKind::FalseKeyword as u16 => Some("false"),
                            k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                            k if k == SyntaxKind::NumericLiteral as u16 => Some("number"),
                            k if k == SyntaxKind::StringLiteral as u16 => Some("string"),
                            // Also check for identifiers with reserved names (parsed as identifier)
                            k if k == SyntaxKind::Identifier as u16 => {
                                if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                                    match ident.escaped_text.as_str() {
                                        "undefined" => Some("undefined"),
                                        "null" => Some("null"),
                                        "void" => Some("void"),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };

                        if let Some(type_name) = literal_type_name {
                            if is_extends_clause {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[type_name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                );
                            }
                            continue;
                        }
                    }
                    // Get the name for the error message
                    if let Some(name) = self.heritage_name_text(expr_idx) {
                        if matches!(
                            name.as_str(),
                            "undefined" | "null" | "true" | "false" | "void" | "0"
                        ) {
                            continue;
                        }
                        if self.is_known_global_type_name(&name) {
                            continue;
                        }
                        self.error_cannot_find_name_at(&name, expr_idx);
                    }
                }
            }
        }
    }

    /// Check a class declaration.
    fn check_class_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(class) = self.ctx.arena.get_class(node) else {
            return;
        };

        // Check for reserved class names (error 2414)
        if !class.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    if ident.escaped_text == "any" {
                        self.error_at_node(
                            class.name,
                            "Class name cannot be 'any'.",
                            diagnostic_codes::CLASS_NAME_CANNOT_BE_ANY,
                        );
                    }
                }
            }
        }

        // Check if this is a declared class (ambient declaration)
        let is_declared = self.has_declare_modifier(&class.modifiers);

        // Check if this class is abstract
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        // Push type parameters BEFORE checking heritage clauses and abstract members
        // This allows heritage clauses and member checks to reference the class's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(&class.heritage_clauses);

        // Check for abstract members in non-abstract class (error 1253)
        // and private identifiers in ambient classes (error 2819)
        for &member_idx in &class.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                // TS2819: Check for private identifiers in ambient classes
                if is_declared {
                    let member_name_idx = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => self
                            .ctx
                            .arena
                            .get_property_decl(member_node)
                            .map(|p| p.name),
                        syntax_kind_ext::METHOD_DECLARATION => {
                            self.ctx.arena.get_method_decl(member_node).map(|m| m.name)
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            self.ctx.arena.get_accessor(member_node).map(|a| a.name)
                        }
                        _ => None,
                    };

                    if let Some(name_idx) = member_name_idx {
                        if !name_idx.is_none() {
                            if let Some(name_node) = self.ctx.arena.get(name_idx) {
                                if name_node.kind
                                    == crate::scanner::SyntaxKind::PrivateIdentifier as u16
                                {
                                    use crate::checker::types::diagnostics::diagnostic_messages;
                                    self.error_at_node(
                                        name_idx,
                                        diagnostic_messages::PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT,
                                        diagnostic_codes::PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT,
                                    );
                                }
                            }
                        }
                    }
                }

                // Check for abstract members in non-abstract class
                if !is_abstract_class {
                    let member_has_abstract = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => {
                            if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                                self.has_abstract_modifier(&prop.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                                self.has_abstract_modifier(&method.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                                self.has_abstract_modifier(&accessor.modifiers)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if member_has_abstract {
                        // Report on the 'abstract' keyword
                        self.error_at_node(
                            member_idx,
                            "Abstract properties can only appear within an abstract class.",
                            diagnostic_codes::ABSTRACT_ONLY_IN_ABSTRACT_CLASS,
                        );
                    }
                }
            }
        }

        // Collect class name and static members for error 2662 suggestions
        let class_name = if !class.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    Some(ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Save previous enclosing class and set current
        let prev_enclosing_class = self.ctx.enclosing_class.take();
        if let Some(name) = class_name {
            self.ctx.enclosing_class = Some(EnclosingClassInfo {
                name,
                class_idx: stmt_idx,
                member_nodes: class.members.nodes.clone(),
                in_constructor: false,
                is_declared,
            });
        }

        // Check each class member
        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        // Check for missing method/constructor implementations (2389, 2390, 2391)
        // Skip for declared classes (ambient declarations don't need implementations)
        if !is_declared {
            self.check_class_member_implementations(&class.members.nodes);
        }

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check for getter/setter type compatibility (error 2322)
        // Getter return type must be assignable to setter parameter type
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check strict property initialization (TS2564)
        self.check_property_initialization(stmt_idx, &class, is_declared, is_abstract_class);

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(stmt_idx, &class);

        // Check that non-abstract class implements all abstract members from base class (error 2654)
        self.check_abstract_member_implementations(stmt_idx, &class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(stmt_idx, &class);

        // Restore previous enclosing class
        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::thin_node::ClassData,
    ) {
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        let class_name = self.get_class_name_from_decl(class_idx);
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        let prev_enclosing_class = self.ctx.enclosing_class.take();
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared: false,
        });

        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        // Check strict property initialization (TS2564) for class expressions
        // Class expressions should have the same property initialization checks as class declarations
        self.check_property_initialization(class_idx, class, false, is_abstract_class);

        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    fn check_property_initialization(
        &mut self,
        _class_idx: NodeIndex,
        class: &crate::parser::thin_node::ClassData,
        is_declared: bool,
        is_abstract: bool,
    ) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip TS2564 for declared classes (ambient declarations)
        // Note: Abstract classes DO get TS2564 errors - they can have constructors
        // and properties must be initialized either with defaults or in the constructor
        if is_declared {
            return;
        }

        // Only check property initialization when strictPropertyInitialization is enabled
        if !self.ctx.strict_property_initialization {
            return;
        }

        let mut properties = Vec::new();
        let mut tracked = FxHashSet::default();
        let mut parameter_properties = FxHashSet::default();

        // First pass: collect parameter properties from constructor
        // Parameter properties are always definitely assigned
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };

            // Collect parameter properties from constructor parameters
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Parameter properties have modifiers (public/private/protected/readonly)
                if param.modifiers.is_some() {
                    if let Some(key) = self.property_key_from_name(param.name) {
                        parameter_properties.insert(key.clone());
                    }
                }
            }
        }

        // Second pass: collect class properties that need initialization
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(node) else {
                continue;
            };

            if !self.property_requires_initialization(member_idx, prop) {
                continue;
            }

            let Some(key) = self.property_key_from_name(prop.name) else {
                continue;
            };

            // Get property name for error message. Use fallback for complex computed properties.
            let name = self.get_property_name(prop.name).unwrap_or_else(|| {
                // For complex computed properties (e.g., [getKey()]), use a descriptive fallback
                match &key {
                    PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{}]", s),
                    PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{}\"]", s),
                    PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{}]", n),
                    PropertyKey::Computed(ComputedKey::Qualified(q)) => format!("[{}]", q),
                    PropertyKey::Computed(ComputedKey::Symbol(Some(s))) => format!("[Symbol({})]", s),
                    PropertyKey::Computed(ComputedKey::Symbol(None)) => "[Symbol()]".to_string(),
                    PropertyKey::Private(s) => format!("#{}", s),
                    PropertyKey::Ident(s) => s.clone(),
                }
            });

            tracked.insert(key.clone());
            properties.push((key, name, prop.name));
        }

        if properties.is_empty() {
            return;
        }

        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);
        let assigned = if let Some(body_idx) = constructor_body {
            self.analyze_constructor_assignments(body_idx, &tracked, requires_super)
        } else {
            FxHashSet::default()
        };

        for (key, name, name_node) in properties {
            // Property is assigned if it's in the assigned set OR it's a parameter property
            if assigned.contains(&key) || parameter_properties.contains(&key) {
                continue;
            }
            use crate::checker::types::diagnostics::format_message;
            self.error_at_node(
                name_node,
                &format_message(diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER, &[&name]),
                diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER,
            );
        }

        // Check for TS2565 (Property used before being assigned in constructor)
        if let Some(body_idx) = constructor_body {
            self.check_properties_used_before_assigned(body_idx, &tracked, requires_super);
        }
    }

    fn property_requires_initialization(
        &mut self,
        member_idx: NodeIndex,
        prop: &crate::parser::thin_node::PropertyDeclData,
    ) -> bool {
        use crate::scanner::SyntaxKind;

        if !prop.initializer.is_none()
            || prop.question_token
            || prop.exclamation_token
            || self.has_static_modifier(&prop.modifiers)
            || self.has_abstract_modifier(&prop.modifiers)
            || self.has_declare_modifier(&prop.modifiers)
        {
            return false;
        }

        // Properties with string or numeric literal names are not checked for strict property initialization
        // Example: class C { "b": number; 0: number; }  // These are not checked
        let Some(name_node) = self.ctx.arena.get(prop.name) else {
            return false;
        };
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        let prop_type = if !prop.type_annotation.is_none() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ANY
        };

        if prop_type == TypeId::ANY || prop_type == TypeId::UNKNOWN {
            return false;
        }

        !self.type_includes_undefined(prop_type)
    }

    fn class_has_base(&self, class: &crate::parser::thin_node::ClassData) -> bool {
        use crate::scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return true;
            }
        }

        false
    }

    fn type_includes_undefined(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        if type_id == TypeId::UNDEFINED {
            return true;
        }

        let Some(TypeKey::Union(members)) = self.ctx.types.lookup(type_id) else {
            return false;
        };

        let members = self.ctx.types.type_list(members);
        members.iter().any(|&member| member == TypeId::UNDEFINED)
    }

    fn find_constructor_body(&self, members: &crate::parser::NodeList) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };
            if !ctor.body.is_none() {
                return Some(ctor.body);
            }
        }
        None
    }

    /// Check for TS2565: Properties used before being assigned in the constructor.
    ///
    /// This function analyzes the constructor body to detect when a property
    /// is accessed (via `this.X`) before it has been assigned a value.
    fn check_properties_used_before_assigned(
        &mut self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) {
        if body_idx.is_none() {
            return;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        let start_idx = if require_super {
            self.find_super_statement_start(&block.statements.nodes)
                .unwrap_or(0)
        } else {
            0
        };

        let mut assigned = FxHashSet::default();

        // Track parameter properties as already assigned
        for _key in tracked.iter() {
            // Parameter properties are assigned in the parameter list
            // We'll collect them separately if needed
        }

        // Analyze statements in order, checking for property accesses before assignment
        for &stmt_idx in block.statements.nodes.iter().skip(start_idx) {
            self.check_statement_for_early_property_access(
                stmt_idx,
                &mut assigned,
                tracked,
            );
        }
    }

    /// Check a single statement for property accesses that occur before assignment.
    /// Returns true if the statement definitely assigns to the tracked property.
    fn check_statement_for_early_property_access(
        &mut self,
        stmt_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> bool {
        if stmt_idx.is_none() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement_for_early_property_access(
                            stmt_idx, assigned, tracked,
                        );
                    }
                }
                false
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.check_expression_for_early_property_access(
                        expr_stmt.expression, assigned, tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    // Check the condition expression for property accesses
                    self.check_expression_for_early_property_access(
                        if_stmt.expression, assigned, tracked,
                    );
                    // Check both branches
                    let mut then_assigned = assigned.clone();
                    let mut else_assigned = assigned.clone();
                    self.check_statement_for_early_property_access(
                        if_stmt.then_statement, &mut then_assigned, tracked,
                    );
                    if !if_stmt.else_statement.is_none() {
                        self.check_statement_for_early_property_access(
                            if_stmt.else_statement, &mut else_assigned, tracked,
                        );
                    }
                    // Properties assigned in both branches are considered assigned
                    *assigned = then_assigned
                        .intersection(&else_assigned)
                        .cloned()
                        .collect();
                }
                false
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret_stmt) = self.ctx.arena.get_return_statement(node) {
                    if !ret_stmt.expression.is_none() {
                        self.check_expression_for_early_property_access(
                            ret_stmt.expression, assigned, tracked,
                        );
                    }
                }
                false
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                // For loops, we conservatively don't track assignments across iterations
                // This is a simplified approach - the full TypeScript implementation is more complex
                false
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.ctx.arena.get_try(node) {
                    self.check_statement_for_early_property_access(
                        try_stmt.try_block, assigned, tracked,
                    );
                    // Check catch and finally blocks
                    // ...
                }
                false
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &decl_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                            if let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) {
                                if !decl.initializer.is_none() {
                                    self.check_expression_for_early_property_access(
                                        decl.initializer, assigned, tracked,
                                    );
                                }
                            }
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Check an expression for property accesses that occur before assignment.
    fn check_expression_for_early_property_access(
        &mut self,
        expr_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if expr_idx.is_none() {
            return;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                // Check if this is a this.X access
                if let Some(key) = self.property_key_from_access(expr_idx) {
                    // Check if this is a property read (not an assignment)
                    // We need to look at the parent to determine if this is the target of an assignment
                    // For now, we'll check if the property is being read before assignment
                    if tracked.contains(&key) && !assigned.contains(&key) {
                        // Emit TS2565 error
                        use crate::checker::types::diagnostics::format_message;
                        let property_name = self.get_property_name_from_key(&key);
                        self.error_at_node(
                            expr_idx,
                            &format_message(
                                crate::checker::types::diagnostics::diagnostic_messages::PROPERTY_USED_BEFORE_BEING_ASSIGNED,
                                &[&property_name],
                            ),
                            crate::checker::types::diagnostics::diagnostic_codes::PROPERTY_USED_BEFORE_BEING_ASSIGNED,
                        );
                    }
                }
                // Recursively check the expression part
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.check_expression_for_early_property_access(
                        access.expression, assigned, tracked,
                    );
                    if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                        self.check_expression_for_early_property_access(
                            access.name_or_argument, assigned, tracked,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                    // Check both sides of the binary expression
                    self.check_expression_for_early_property_access(
                        bin.left, assigned, tracked,
                    );
                    self.check_expression_for_early_property_access(
                        bin.right, assigned, tracked,
                    );
                    // If this is an assignment, track the assignment
                    if self.is_assignment_operator(bin.operator_token) {
                        self.track_assignment_in_expression(bin.left, assigned, tracked);
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.check_expression_for_early_property_access(
                        unary.operand, assigned, tracked,
                    );
                    // Track ++ and -- as both read and write
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        self.track_assignment_in_expression(unary.operand, assigned, tracked);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION =>
            {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.check_expression_for_early_property_access(
                        call.expression, assigned, tracked,
                    );
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.check_expression_for_early_property_access(
                                arg, assigned, tracked,
                            );
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.check_expression_for_early_property_access(
                        cond.condition, assigned, tracked,
                    );
                    self.check_expression_for_early_property_access(
                        cond.when_true, assigned, tracked,
                    );
                    self.check_expression_for_early_property_access(
                        cond.when_false, assigned, tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.check_expression_for_early_property_access(
                        paren.expression, assigned, tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.check_expression_for_early_property_access(
                        assertion.expression, assigned, tracked,
                    );
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_expression_for_early_property_access(
                        unary.expression, assigned, tracked,
                    );
                }
            }
            _ => {}
        }
    }

    /// Track property assignments in an expression.
    fn track_assignment_in_expression(
        &self,
        target_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if target_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(key) = self.property_key_from_access(target_idx) {
                    if tracked.contains(&key) {
                        assigned.insert(key);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.track_assignment_in_expression(paren.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.track_assignment_in_expression(assertion.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.track_assignment_in_expression(unary.expression, assigned, tracked);
                }
            }
            _ => {}
        }
    }

    /// Get property name as a string for error messages.
    fn get_property_name_from_key(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::Ident(s) => s.clone(),
            PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{}]", s),
            PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{}\"]", s),
            PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{}]", n),
            PropertyKey::Computed(ComputedKey::Qualified(q)) => format!("[{}]", q),
            PropertyKey::Computed(ComputedKey::Symbol(Some(s))) => format!("[Symbol({})]", s),
            PropertyKey::Computed(ComputedKey::Symbol(None)) => "[Symbol()]".to_string(),
            PropertyKey::Private(s) => format!("#{}", s),
        }
    }

    fn analyze_constructor_assignments(
        &self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) -> FxHashSet<PropertyKey> {
        let result = if require_super {
            self.analyze_constructor_body_after_super(body_idx, tracked)
        } else {
            self.analyze_statement(body_idx, &FxHashSet::default(), tracked)
        };

        self.flow_result_to_assigned(result)
    }

    fn analyze_constructor_body_after_super(
        &self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        let Some(start_idx) = self.find_super_statement_start(&block.statements.nodes) else {
            return FlowResult {
                normal: Some(FxHashSet::default()),
                exits: None,
            };
        };

        self.analyze_block(
            &block.statements.nodes[start_idx..],
            &FxHashSet::default(),
            tracked,
        )
    }

    fn find_super_statement_start(&self, statements: &[NodeIndex]) -> Option<usize> {
        for (idx, &stmt_idx) in statements.iter().enumerate() {
            if self.is_super_call_statement(stmt_idx) {
                return Some(idx + 1);
            }
        }
        None
    }

    fn is_super_call_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        callee_node.kind == SyntaxKind::SuperKeyword as u16
    }

    fn flow_result_to_assigned(&self, result: FlowResult) -> FxHashSet<PropertyKey> {
        let mut assigned = None;
        if let Some(normal) = result.normal {
            assigned = Some(normal);
        }
        if let Some(exits) = result.exits {
            assigned = Some(match assigned {
                Some(current) => self.intersect_sets(&current, &exits),
                None => exits,
            });
        }

        assigned.unwrap_or_default()
    }

    fn analyze_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        if stmt_idx.is_none() {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    return self.analyze_block(&block.statements.nodes, assigned_in, tracked);
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    let mut assigned = assigned_in.clone();
                    self.collect_assignments_in_expression(
                        if_stmt.expression,
                        &mut assigned,
                        tracked,
                    );

                    let then_result =
                        self.analyze_statement(if_stmt.then_statement, &assigned, tracked);

                    let else_result = if !if_stmt.else_statement.is_none() {
                        self.analyze_statement(if_stmt.else_statement, &assigned, tracked)
                    } else {
                        FlowResult {
                            normal: Some(assigned),
                            exits: None,
                        }
                    };

                    return FlowResult {
                        normal: self.combine_flow_sets(then_result.normal, else_result.normal),
                        exits: self.combine_flow_sets(then_result.exits, else_result.exits),
                    };
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(ret) = self.ctx.arena.get_return_statement(node) {
                    if !ret.expression.is_none() {
                        self.collect_assignments_in_expression(
                            ret.expression,
                            &mut assigned,
                            tracked,
                        );
                    }
                }
                return FlowResult {
                    normal: None,
                    exits: Some(assigned),
                };
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                let mut assigned = assigned_in.clone();
                if let Some(ret) = self.ctx.arena.get_return_statement(node) {
                    if !ret.expression.is_none() {
                        self.collect_assignments_in_expression(
                            ret.expression,
                            &mut assigned,
                            tracked,
                        );
                    }
                }
                return FlowResult {
                    normal: None,
                    exits: None,
                };
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr) = self.ctx.arena.get_expression_statement(node) {
                    let mut assigned = assigned_in.clone();
                    self.collect_assignments_in_expression(expr.expression, &mut assigned, tracked);
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    let mut assigned = assigned_in.clone();
                    if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                        self.collect_assignments_in_variable_decl_list(
                            decl_list_idx,
                            &mut assigned,
                            tracked,
                        );
                    }
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    return self.analyze_try_statement(try_data, assigned_in, tracked);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node) {
                    return self.analyze_switch_statement(switch_data, assigned_in, tracked);
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT =>
            {
                // For while/for loops: body might not execute, so assignments
                // in the body don't count for definite assignment
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    let mut assigned = assigned_in.clone();
                    if !loop_data.initializer.is_none() {
                        if let Some(init_node) = self.ctx.arena.get(loop_data.initializer) {
                            if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                self.collect_assignments_in_variable_decl_list(
                                    loop_data.initializer,
                                    &mut assigned,
                                    tracked,
                                );
                            } else {
                                self.collect_assignments_in_expression(
                                    loop_data.initializer,
                                    &mut assigned,
                                    tracked,
                                );
                            }
                        }
                    }
                    if !loop_data.condition.is_none() {
                        self.collect_assignments_in_expression(
                            loop_data.condition,
                            &mut assigned,
                            tracked,
                        );
                    }
                    if !loop_data.incrementor.is_none() {
                        self.collect_assignments_in_expression(
                            loop_data.incrementor,
                            &mut assigned,
                            tracked,
                        );
                    }
                    // Note: We deliberately DON'T analyze the loop body for definite assignment
                    // because while/for loops might not execute at all
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::DO_STATEMENT =>
            {
                // do-while loops always execute at least once
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    let mut assigned = assigned_in.clone();
                    // Analyze the loop body (executes at least once)
                    let body_result = self.analyze_statement(loop_data.statement, &assigned, tracked);
                    if !loop_data.condition.is_none() {
                        self.collect_assignments_in_expression(
                            loop_data.condition,
                            &mut assigned,
                            tracked,
                        );
                    }
                    // Use the assignments from the body
                    return FlowResult {
                        normal: body_result.normal,
                        exits: None,
                    };
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_data) = self.ctx.arena.get_for_in_of(node) {
                    let mut assigned = assigned_in.clone();
                    if !for_data.initializer.is_none() {
                        if let Some(init_node) = self.ctx.arena.get(for_data.initializer) {
                            if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                self.collect_assignments_in_variable_decl_list(
                                    for_data.initializer,
                                    &mut assigned,
                                    tracked,
                                );
                            } else {
                                self.collect_assignments_in_expression(
                                    for_data.initializer,
                                    &mut assigned,
                                    tracked,
                                );
                            }
                        }
                    }
                    if !for_data.expression.is_none() {
                        self.collect_assignments_in_expression(
                            for_data.expression,
                            &mut assigned,
                            tracked,
                        );
                    }
                    return FlowResult {
                        normal: Some(assigned),
                        exits: None,
                    };
                }
            }
            _ => {}
        }

        FlowResult {
            normal: Some(assigned_in.clone()),
            exits: None,
        }
    }

    fn analyze_block(
        &self,
        statements: &[NodeIndex],
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let mut assigned = assigned_in.clone();
        let mut normal = Some(assigned.clone());
        let mut exits: Option<FxHashSet<PropertyKey>> = None;

        for &stmt_idx in statements {
            if normal.is_none() {
                break;
            }
            let result = self.analyze_statement(stmt_idx, &assigned, tracked);
            exits = self.combine_flow_sets(exits, result.exits);
            match result.normal {
                Some(next) => {
                    assigned = next;
                    normal = Some(assigned.clone());
                }
                None => {
                    normal = None;
                }
            }
        }

        FlowResult { normal, exits }
    }

    fn analyze_try_statement(
        &self,
        try_data: &crate::parser::thin_node::TryData,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let try_result = self.analyze_statement(try_data.try_block, assigned_in, tracked);
        let catch_result = if !try_data.catch_clause.is_none() {
            if let Some(catch_node) = self.ctx.arena.get(try_data.catch_clause) {
                if let Some(catch) = self.ctx.arena.get_catch_clause(catch_node) {
                    self.analyze_statement(catch.block, assigned_in, tracked)
                } else {
                    FlowResult {
                        normal: None,
                        exits: None,
                    }
                }
            } else {
                FlowResult {
                    normal: None,
                    exits: None,
                }
            }
        } else {
            FlowResult {
                normal: None,
                exits: None,
            }
        };

        let mut normal = if try_data.catch_clause.is_none() {
            try_result.normal
        } else {
            self.combine_flow_sets(try_result.normal, catch_result.normal)
        };
        let mut exits = if try_data.catch_clause.is_none() {
            try_result.exits
        } else {
            self.combine_flow_sets(try_result.exits, catch_result.exits)
        };

        if !try_data.finally_block.is_none() {
            let finally_result =
                self.analyze_statement(try_data.finally_block, &FxHashSet::default(), tracked);
            let finally_assigned = self
                .combine_flow_sets(finally_result.normal, finally_result.exits)
                .unwrap_or_default();

            if let Some(ref mut normal_set) = normal {
                normal_set.extend(finally_assigned.iter().cloned());
            }
            if let Some(ref mut exits_set) = exits {
                exits_set.extend(finally_assigned.iter().cloned());
            }
        }

        FlowResult { normal, exits }
    }

    fn analyze_switch_statement(
        &self,
        switch_data: &crate::parser::thin_node::SwitchData,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let mut assigned = assigned_in.clone();
        self.collect_assignments_in_expression(switch_data.expression, &mut assigned, tracked);

        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return FlowResult {
                normal: Some(assigned),
                exits: None,
            };
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return FlowResult {
                normal: Some(assigned),
                exits: None,
            };
        };

        let mut normal: Option<FxHashSet<PropertyKey>> = None;
        let mut exits: Option<FxHashSet<PropertyKey>> = None;

        let mut has_default_clause = false;

        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if let Some(clause) = self.ctx.arena.get_case_clause(clause_node) {
                // Check if this is a default clause (no expression)
                if clause.expression.is_none() {
                    has_default_clause = true;
                }
                let result = self.analyze_block(&clause.statements.nodes, &assigned, tracked);
                normal = self.combine_flow_sets(normal, result.normal);
                exits = self.combine_flow_sets(exits, result.exits);
            }
        }

        // If there's no default clause, the switch might not execute any case
        // Properties are only definitely assigned if ALL cases assign them
        // AND the switch covers all possible values (has default)
        if !has_default_clause {
            // Without a default, we can't guarantee any case will execute
            // Return empty normal flow to indicate properties are not definitely assigned
            return FlowResult {
                normal: None,
                exits: Some(assigned.clone()),
            };
        }

        // With a default clause, use the combined assignments
        if normal.is_none() && exits.is_some() {
            normal = exits.clone();
        } else if normal.is_none() && exits.is_none() {
            normal = Some(assigned);
        }

        FlowResult { normal, exits }
    }

    fn collect_assignments_in_variable_decl_list(
        &self,
        decl_list_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };
        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !decl.initializer.is_none() {
                self.collect_assignments_in_expression(decl.initializer, assigned, tracked);
            }
        }
    }

    fn collect_assignments_in_expression(
        &self,
        expr_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if expr_idx.is_none() {
            return;
        }

        let mut stack = vec![expr_idx];
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };

            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    continue;
                }
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                        if self.is_assignment_operator(bin.operator_token) {
                            self.collect_assignment_target(bin.left, assigned, tracked);
                        }
                        if !bin.right.is_none() {
                            stack.push(bin.right);
                        }
                        if !bin.left.is_none() {
                            stack.push(bin.left);
                        }
                    }
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
                {
                    if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                        if unary.operator == SyntaxKind::PlusPlusToken as u16
                            || unary.operator == SyntaxKind::MinusMinusToken as u16
                        {
                            self.collect_assignment_target(unary.operand, assigned, tracked);
                        }
                        if !unary.operand.is_none() {
                            stack.push(unary.operand);
                        }
                    }
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        stack.push(call.expression);
                        if let Some(ref args) = call.arguments {
                            for &arg in &args.nodes {
                                stack.push(arg);
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    if let Some(access) = self.ctx.arena.get_access_expr(node) {
                        stack.push(access.expression);
                        stack.push(access.name_or_argument);
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        stack.push(paren.expression);
                    }
                }
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        stack.push(cond.condition);
                        stack.push(cond.when_true);
                        stack.push(cond.when_false);
                    }
                }
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                {
                    if let Some(literal) = self.ctx.arena.get_literal_expr(node) {
                        for &elem in &literal.elements.nodes {
                            stack.push(elem);
                        }
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                        stack.push(prop.initializer);
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ELEMENT
                    || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
                {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        stack.push(spread.expression);
                    }
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        stack.push(assertion.expression);
                    }
                }
                k if k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::AWAIT_EXPRESSION
                    || k == syntax_kind_ext::YIELD_EXPRESSION =>
                {
                    if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                        stack.push(unary.expression);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_assignment_target(
        &self,
        target_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if target_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(key) = self.property_key_from_access(target_idx) {
                    self.record_property_assignment(key, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_assignment_target(paren.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.collect_assignment_target(assertion.expression, assigned, tracked);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.collect_assignment_target(unary.expression, assigned, tracked);
                }
            }
            // Handle destructuring assignments: ({ a: this.a, b: this.b } = obj)
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.collect_destructuring_assignments(target_idx, assigned, tracked);
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.collect_array_destructuring_assignments(target_idx, assigned, tracked);
            }
            _ => {}
        }
    }

    /// Collect property assignments from object destructuring patterns.
    /// Handles: ({ a: this.a, b: this.b } = data)
    fn collect_destructuring_assignments(
        &self,
        literal_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(node) = self.ctx.arena.get(literal_idx) else {
            return;
        };
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return;
        };

        for &elem_idx in &literal.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Handle property assignment: { a: this.a }
            if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    // Check if the value being assigned is a property access like this.a
                    if let Some(key) = self.property_key_from_access(prop.initializer) {
                        self.record_property_assignment(key, assigned, tracked);
                    }
                }
            }
            // Handle shorthand property assignment: { this.a }
            // (This is less common but syntactically valid in destructuring)
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) {
                    if let Some(key) = self.property_key_from_access(prop.name) {
                        self.record_property_assignment(key, assigned, tracked);
                    }
                }
            }
            // Handle nested destructuring (recursively)
            else if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                self.collect_destructuring_assignments(elem_idx, assigned, tracked);
            }
            else if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                self.collect_array_destructuring_assignments(elem_idx, assigned, tracked);
            }
        }
    }

    /// Collect property assignments from array destructuring patterns.
    /// Handles: [this.a, this.b] = arr
    fn collect_array_destructuring_assignments(
        &self,
        literal_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        let Some(node) = self.ctx.arena.get(literal_idx) else {
            return;
        };
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return;
        };

        for &elem_idx in &literal.elements.nodes {
            // Skip holes in array destructuring: [a, , b]
            if elem_idx.is_none() {
                continue;
            }

            // Check if the element is a property access like this.a
            if let Some(key) = self.property_key_from_access(elem_idx) {
                self.record_property_assignment(key, assigned, tracked);
            }
            // Handle nested destructuring
            else if let Some(elem_node) = self.ctx.arena.get(elem_idx) {
                if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.collect_destructuring_assignments(elem_idx, assigned, tracked);
                } else if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.collect_array_destructuring_assignments(elem_idx, assigned, tracked);
                }
            }
        }
    }

    fn record_property_assignment(
        &self,
        key: PropertyKey,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) {
        if tracked.contains(&key) {
            assigned.insert(key.clone());
        }

        match key {
            PropertyKey::Ident(name) => {
                let computed = PropertyKey::Computed(ComputedKey::String(name));
                if tracked.contains(&computed) {
                    assigned.insert(computed);
                }
            }
            PropertyKey::Computed(ComputedKey::String(name)) => {
                let ident = PropertyKey::Ident(name);
                if tracked.contains(&ident) {
                    assigned.insert(ident);
                }
            }
            _ => {}
        }
    }

    fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        use crate::scanner::SyntaxKind;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                return Some(PropertyKey::Private(ident.escaped_text.clone()));
            }
            return Some(PropertyKey::Ident(ident.escaped_text.clone()));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                if !lit.text.is_empty() {
                    let key = if name_node.kind == SyntaxKind::NumericLiteral as u16 {
                        PropertyKey::Computed(ComputedKey::Number(lit.text.clone()))
                    } else {
                        PropertyKey::Computed(ComputedKey::String(lit.text.clone()))
                    };
                    return Some(key);
                }
            }
        }

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.ctx.arena.get_computed_property(name_node) {
                return self
                    .computed_key_from_expression(computed.expression)
                    .map(PropertyKey::Computed);
            }
        }

        None
    }

    fn property_key_from_access(&self, access_idx: NodeIndex) -> Option<PropertyKey> {
        let Some(node) = self.ctx.arena.get(access_idx) else {
            return None;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return None;
        };
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return None;
        };
        if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
                return None;
            };
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                    return Some(PropertyKey::Private(ident.escaped_text.clone()));
                }
                return Some(PropertyKey::Ident(ident.escaped_text.clone()));
            }
            return None;
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return self
                .computed_key_from_expression(access.name_or_argument)
                .map(PropertyKey::Computed);
        }

        None
    }

    fn computed_key_from_expression(&self, expr_idx: NodeIndex) -> Option<ComputedKey> {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            return Some(ComputedKey::Ident(ident.escaped_text.clone()));
        }

        if let Some(lit) = self.ctx.arena.get_literal(expr_node) {
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    return Some(ComputedKey::String(lit.text.clone()));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    return Some(ComputedKey::Number(lit.text.clone()));
                }
                _ => {}
            }
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access_name) = self.qualified_name_from_property_access(expr_idx) {
                return Some(ComputedKey::Qualified(access_name));
            }
        }

        // Handle call expressions like Symbol("key")
        if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.ctx.arena.get_call_expr(expr_node) {
                // Check if callee is "Symbol"
                if let Some(callee_node) = self.ctx.arena.get(call.expression) {
                    if let Some(callee_ident) = self.ctx.arena.get_identifier(callee_node) {
                        if callee_ident.escaped_text == "Symbol" {
                            // Try to get the description argument if present
                            let description = call
                                .arguments
                                .as_ref()
                                .and_then(|args| args.nodes.first())
                                .and_then(|&first_arg| self.ctx.arena.get(first_arg))
                                .and_then(|arg_node| {
                                    if arg_node.kind == SyntaxKind::StringLiteral as u16 {
                                        self.ctx.arena.get_literal(arg_node).map(|lit| lit.text.clone())
                                    } else {
                                        None
                                    }
                                });
                            return Some(ComputedKey::Symbol(description));
                        }
                    }
                }
            }
        }

        None
    }

    fn qualified_name_from_property_access(&self, access_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(access_idx) else {
            return None;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return None;
        };

        let base_name = if let Some(base_node) = self.ctx.arena.get(access.expression) {
            if let Some(ident) = self.ctx.arena.get_identifier(base_node) {
                Some(ident.escaped_text.clone())
            } else if base_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.qualified_name_from_property_access(access.expression)
            } else {
                None
            }
        } else {
            None
        }?;

        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return None;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return None;
        };

        Some(format!("{}.{}", base_name, ident.escaped_text))
    }

    fn is_assignment_operator(&self, operator: u16) -> bool {
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

    fn skip_parenthesized_expression(&self, mut expr_idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.ctx.arena.get(expr_idx) {
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                break;
            }
            let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                break;
            };
            expr_idx = paren.expression;
        }
        expr_idx
    }

    fn is_side_effect_free(&self, expr_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let expr_idx = self.skip_parenthesized_expression(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_ELEMENT =>
            {
                true
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
                    return false;
                };
                self.is_side_effect_free(cond.when_true)
                    && self.is_side_effect_free(cond.when_false)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(bin) = self.ctx.arena.get_binary_expr(node) else {
                    return false;
                };
                if self.is_assignment_operator(bin.operator_token) {
                    return false;
                }
                self.is_side_effect_free(bin.left) && self.is_side_effect_free(bin.right)
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
                    return false;
                };
                matches!(
                    unary.operator,
                    k if k == SyntaxKind::ExclamationToken as u16
                        || k == SyntaxKind::PlusToken as u16
                        || k == SyntaxKind::MinusToken as u16
                        || k == SyntaxKind::TildeToken as u16
                        || k == SyntaxKind::TypeOfKeyword as u16
                )
            }
            _ => false,
        }
    }

    fn is_numeric_literal_zero(&self, expr_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::NumericLiteral as u16 {
            return false;
        }
        let Some(lit) = self.ctx.arena.get_literal(node) else {
            return false;
        };
        lit.text == "0"
    }

    fn is_access_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        )
    }

    fn is_indirect_call(&self, comma_idx: NodeIndex, left: NodeIndex, right: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let parent = self
            .ctx
            .arena
            .get_extended(comma_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        if !self.is_numeric_literal_zero(left) {
            return false;
        }

        let grand_parent = self
            .ctx
            .arena
            .get_extended(parent)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        if grand_parent.is_none() {
            return false;
        }
        let Some(grand_node) = self.ctx.arena.get(grand_parent) else {
            return false;
        };

        let is_indirect_target = if grand_node.kind == syntax_kind_ext::CALL_EXPRESSION {
            if let Some(call) = self.ctx.arena.get_call_expr(grand_node) {
                call.expression == parent
            } else {
                false
            }
        } else if grand_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            if let Some(tagged) = self.ctx.arena.get_tagged_template(grand_node) {
                tagged.tag == parent
            } else {
                false
            }
        } else {
            false
        };
        if !is_indirect_target {
            return false;
        }

        if self.is_access_expression(right) {
            return true;
        }
        let Some(right_node) = self.ctx.arena.get(right) else {
            return false;
        };
        if right_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier(right_node) else {
            return false;
        };
        ident.escaped_text == "eval"
    }

    fn combine_flow_sets(
        &self,
        left: Option<FxHashSet<PropertyKey>>,
        right: Option<FxHashSet<PropertyKey>>,
    ) -> Option<FxHashSet<PropertyKey>> {
        match (left, right) {
            (Some(a), Some(b)) => Some(self.intersect_sets(&a, &b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn intersect_sets(
        &self,
        left: &FxHashSet<PropertyKey>,
        right: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        if left.len() <= right.len() {
            left.iter()
                .filter(|key| right.contains(*key))
                .cloned()
                .collect()
        } else {
            right
                .iter()
                .filter(|key| left.contains(*key))
                .cloned()
                .collect()
        }
    }

    /// Check an interface declaration.
    fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // Check for reserved interface names (error 2427)
        if !iface.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(iface.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    // Reserved type names that can't be used as interface names
                    match ident.escaped_text.as_str() {
                        "string" | "number" | "boolean" | "symbol" | "void" | "object" => {
                            self.error_at_node(
                                iface.name,
                                &format!("Interface name cannot be '{}'.", ident.escaped_text),
                                diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        // Check heritage clauses for unresolved names (TS2304)
        self.check_heritage_clauses_for_unresolved_names(&iface.heritage_clauses);

        let (_type_params, type_param_updates) = self.push_type_parameters(&iface.type_parameters);

        // Check each interface member for missing type references and parameter properties
        for &member_idx in &iface.members.nodes {
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
        }

        // Check that interface correctly extends base interfaces (error 2430)
        self.check_interface_extension_compatibility(stmt_idx, &iface);

        self.pop_type_parameters(type_param_updates);
    }

    /// Check if a node has the `declare` modifier.
    fn has_declare_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::DeclareKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a node has the `async` modifier.
    fn has_async_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::AsyncKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a node has the `abstract` modifier.
    fn has_abstract_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::AbstractKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if modifiers include the 'static' keyword.
    fn has_static_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if modifiers include the 'private' keyword.
    fn has_private_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::PrivateKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if modifiers include the 'protected' keyword.
    fn has_protected_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> bool {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ProtectedKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_private_identifier_name(&self, name_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;
        let Some(node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        node.kind == SyntaxKind::PrivateIdentifier as u16
    }

    fn member_requires_nominal(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
        name_idx: NodeIndex,
    ) -> bool {
        self.has_private_modifier(modifiers)
            || self.has_protected_modifier(modifiers)
            || self.is_private_identifier_name(name_idx)
    }

    fn member_access_level_from_modifiers(
        &self,
        modifiers: &Option<crate::parser::NodeList>,
    ) -> Option<MemberAccessLevel> {
        if self.has_private_modifier(modifiers) {
            return Some(MemberAccessLevel::Private);
        }
        if self.has_protected_modifier(modifiers) {
            return Some(MemberAccessLevel::Protected);
        }
        None
    }

    fn lookup_member_access_in_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> MemberLookup {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return MemberLookup::NotFound;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return MemberLookup::NotFound;
        };

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) != is_static {
                        continue;
                    }
                    let Some(prop_name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    if prop_name == name {
                        let access_level = if self.is_private_identifier_name(prop.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&prop.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) != is_static {
                        continue;
                    }
                    let Some(method_name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if method_name == name {
                        let access_level = if self.is_private_identifier_name(method.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&method.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) != is_static {
                        continue;
                    }
                    let Some(accessor_name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    if accessor_name == name {
                        let access_level = if self.is_private_identifier_name(accessor.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&accessor.modifiers)
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    if is_static {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(param_name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        if param_name == name {
                            return match self.member_access_level_from_modifiers(&param.modifiers) {
                                Some(level) => MemberLookup::Restricted(level),
                                None => MemberLookup::Public,
                            };
                        }
                    }
                }
                _ => {}
            }
        }

        MemberLookup::NotFound
    }

    fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self.get_class_name_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let Some(base_idx) = self.get_base_class_idx(current) else {
                        return None;
                    };
                    current = base_idx;
                }
            }
        }

        None
    }

    fn resolve_class_for_access(
        &mut self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        if self.is_this_expression(expr_idx) {
            if let Some(ref class_info) = self.ctx.enclosing_class {
                return Some((class_info.class_idx, self.is_constructor_type(object_type)));
            }
        }

        if self.is_super_expression(expr_idx) {
            if let Some(ref class_info) = self.ctx.enclosing_class {
                if let Some(base_idx) = self.get_base_class_idx(class_info.class_idx) {
                    return Some((base_idx, self.is_constructor_type(object_type)));
                }
            }
        }

        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if symbol.flags & symbol_flags::CLASS != 0 {
                    if let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id) {
                        return Some((class_idx, true));
                    }
                }
            }
        }

        if object_type != TypeId::ANY && object_type != TypeId::ERROR {
            if let Some(class_idx) = self.get_class_decl_from_type(object_type) {
                return Some((class_idx, false));
            }
        }

        None
    }

    fn resolve_receiver_class_for_access(
        &self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<NodeIndex> {
        if self.is_this_expression(expr_idx) || self.is_super_expression(expr_idx) {
            return self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        }

        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if symbol.flags & symbol_flags::CLASS != 0 {
                    return self.get_class_declaration_from_symbol(sym_id);
                }
            }
        }

        if object_type != TypeId::ANY && object_type != TypeId::ERROR {
            if let Some(class_idx) = self.get_class_decl_from_type(object_type) {
                return Some(class_idx);
            }
        }

        None
    }

    fn check_property_accessibility(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some((class_idx, is_static)) = self.resolve_class_for_access(object_expr, object_type)
        else {
            return true;
        };
        let Some(access_info) = self.find_member_access_info(class_idx, property_name, is_static)
        else {
            return true;
        };

        let current_class_idx = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        let allowed = match access_info.level {
            MemberAccessLevel::Private => {
                current_class_idx == Some(access_info.declaring_class_idx)
            }
            MemberAccessLevel::Protected => match current_class_idx {
                None => false,
                Some(current_class_idx) => {
                    if current_class_idx == access_info.declaring_class_idx {
                        true
                    } else if !self
                        .is_class_derived_from(current_class_idx, access_info.declaring_class_idx)
                    {
                        false
                    } else {
                        let receiver_class_idx =
                            self.resolve_receiver_class_for_access(object_expr, object_type);
                        receiver_class_idx
                            .map(|receiver| receiver == current_class_idx || self.is_class_derived_from(receiver, current_class_idx))
                            .unwrap_or(false)
                    }
                }
            },
        };

        if allowed {
            return true;
        }

        match access_info.level {
            MemberAccessLevel::Private => {
                let message = format!(
                    "Property '{}' is private and only accessible within class '{}'.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(error_node, &message, diagnostic_codes::PROPERTY_IS_PRIVATE);
            }
            MemberAccessLevel::Protected => {
                let message = format!(
                    "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PROTECTED,
                );
            }
        }

        false
    }

    /// Get the const modifier node from a list of modifiers, if present.
    /// Returns the NodeIndex of the const modifier for error reporting.
    fn get_const_modifier(&self, modifiers: &Option<crate::parser::NodeList>) -> Option<NodeIndex> {
        use crate::scanner::SyntaxKind;
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ConstKeyword as u16 {
                        return Some(mod_idx);
                    }
                }
            }
        }
        None
    }

    /// Check if a member with the given name is static by looking up its symbol flags.
    /// Uses the binder's symbol information for efficient O(1) flag checks.
    fn is_static_member(&self, member_nodes: &[NodeIndex], name: &str) -> bool {
        use crate::binder::symbol_flags;

        for &member_idx in member_nodes {
            // Get symbol for this member
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    // Check if name matches and symbol has STATIC flag
                    if symbol.escaped_name == name && (symbol.flags & symbol_flags::STATIC != 0) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a member with the given name is an abstract property by looking up its symbol flags.
    /// Only checks properties (not methods) because accessing this.abstractMethod() in constructor is allowed.
    fn is_abstract_member(&self, member_nodes: &[NodeIndex], name: &str) -> bool {
        use crate::binder::symbol_flags;

        for &member_idx in member_nodes {
            // Get symbol for this member
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    // Check if name matches and symbol has ABSTRACT flag (property only)
                    if symbol.escaped_name == name
                        && (symbol.flags & symbol_flags::ABSTRACT != 0)
                        && (symbol.flags & symbol_flags::PROPERTY != 0)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Recursively check a type node for parameter properties in function types.
    /// Function types (like `(x: T) => R` or `new (x: T) => R`) cannot have parameter properties.
    fn check_type_for_parameter_properties(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        // Check if this is a function type or constructor type
        if node.kind == syntax_kind_ext::FUNCTION_TYPE
            || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        {
            if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                // Check each parameter for parameter property modifiers
                self.check_parameter_properties(&func_type.parameters.nodes);
                for &param_idx in &func_type.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx) {
                        if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                            if !param.type_annotation.is_none() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(func_type.type_annotation);
            }
        }
        // Check type literals (object types) for call/construct signatures
        else if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                for &member_idx in &type_lit.members.nodes {
                    self.check_type_member_for_parameter_properties(member_idx);
                }
            }
        }
        // Recursively check array types, union types, intersection types, etc.
        else if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(arr) = self.ctx.arena.get_array_type(node) {
                self.check_type_for_parameter_properties(arr.element_type);
            }
        } else if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                for &type_idx in &composite.types.nodes {
                    self.check_type_for_parameter_properties(type_idx);
                }
            }
        } else if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            if let Some(paren) = self.ctx.arena.get_wrapped_type(node) {
                self.check_type_for_parameter_properties(paren.type_node);
            }
        }
    }

    /// Walk a type node and emit TS2304 for unresolved type names inside complex types.
    fn check_type_for_missing_names(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let _ = self.get_type_from_type_reference(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let _ = self.get_type_from_type_query(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let updates =
                        self.push_missing_name_type_parameters(&func_type.type_parameters);
                    self.check_type_parameters_for_missing_names(&func_type.type_parameters);
                    for &param_idx in &func_type.parameters.nodes {
                        self.check_parameter_type_for_missing_names(param_idx);
                    }
                    if !func_type.type_annotation.is_none() {
                        self.check_type_for_missing_names(func_type.type_annotation);
                    }
                    self.pop_type_parameters(updates);
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_for_missing_names(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_tuple_element_for_missing_names(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_for_missing_names(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Check check_type and extends_type first (infer type params not in scope yet)
                    self.check_type_for_missing_names(cond.check_type);
                    self.check_type_for_missing_names(cond.extends_type);

                    // Collect infer type parameters from extends_type and add them to scope for true_type
                    let infer_params = self.collect_infer_type_parameters(cond.extends_type);
                    let mut param_bindings = Vec::new();
                    for param_name in &infer_params {
                        let atom = self.ctx.types.intern_string(param_name);
                        let type_id = self.ctx.types.intern(crate::solver::TypeKey::TypeParameter(
                            crate::solver::TypeParamInfo {
                                name: atom,
                                constraint: None,
                                default: None,
                            },
                        ));
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(param_name.clone(), type_id);
                        param_bindings.push((param_name.clone(), previous));
                    }

                    // Check true_type with infer type parameters in scope
                    self.check_type_for_missing_names(cond.true_type);

                    // Remove infer type parameters from scope
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }

                    // Check false_type (infer type params not in scope)
                    self.check_type_for_missing_names(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    self.check_type_for_missing_names(op.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_for_missing_names(indexed.object_type);
                    self.check_type_for_missing_names(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    let mut param_binding: Option<(String, Option<TypeId>)> = None;
                    if let Some(param_node) = self.ctx.arena.get(mapped.type_parameter) {
                        if let Some(param) = self.ctx.arena.get_type_parameter(param_node) {
                            if let Some(name_node) = self.ctx.arena.get(param.name) {
                                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                                    let name = ident.escaped_text.clone();
                                    let atom = self.ctx.types.intern_string(&name);
                                    let type_id = self.ctx.types.intern(
                                        crate::solver::TypeKey::TypeParameter(
                                            crate::solver::TypeParamInfo {
                                                name: atom,
                                                constraint: None,
                                                default: None,
                                            },
                                        ),
                                    );
                                    let previous =
                                        self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                                    param_binding = Some((name, previous));
                                }
                            }
                        }
                    }
                    if !mapped.name_type.is_none() {
                        self.check_type_for_missing_names(mapped.name_type);
                    }
                    if !mapped.type_node.is_none() {
                        self.check_type_for_missing_names(mapped.type_node);
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    if let Some((name, previous)) = param_binding {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node) {
                    if !pred.type_node.is_none() {
                        self.check_type_for_missing_names(pred.type_node);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_for_missing_names(span.expression);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_type_parameters_for_missing_names(
        &mut self,
        type_parameters: &Option<crate::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        for &param_idx in &list.nodes {
            self.check_type_parameter_node_for_missing_names(param_idx);
        }
    }

    fn push_missing_name_type_parameters(
        &mut self,
        type_parameters: &Option<crate::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>)> {
        use crate::solver::{TypeKey, TypeParamInfo};

        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let mut updates = Vec::new();
        for &param_idx in &list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = ident.escaped_text.clone();
            let atom = self.ctx.types.intern_string(&name);
            let type_id = self.ctx.types.intern(TypeKey::TypeParameter(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
            }));
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
        }

        updates
    }

    fn check_type_parameter_node_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return;
        };
        if !param.constraint.is_none() {
            self.check_type_for_missing_names(param.constraint);
        }
        if !param.default.is_none() {
            self.check_type_for_missing_names(param.default);
        }
    }

    fn check_parameter_type_for_missing_names(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if !param.type_annotation.is_none() {
            self.check_type_for_missing_names(param.type_annotation);
        }
    }

    fn check_tuple_element_for_missing_names(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };
        if elem_node.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER {
            if let Some(member) = self.ctx.arena.get_named_tuple_member(elem_node) {
                self.check_type_for_missing_names(member.type_node);
            }
            return;
        }
        self.check_type_for_missing_names(elem_idx);
    }

    fn check_type_member_for_missing_names(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            let updates = self.push_missing_name_type_parameters(&sig.type_parameters);
            self.check_type_parameters_for_missing_names(&sig.type_parameters);
            if let Some(ref params) = sig.parameters {
                for &param_idx in &params.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
            if !sig.type_annotation.is_none() {
                self.check_type_for_missing_names(sig.type_annotation);
            }
            self.pop_type_parameters(updates);
            return;
        }

        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            for &param_idx in &index_sig.parameters.nodes {
                self.check_parameter_type_for_missing_names(param_idx);
            }
            if !index_sig.type_annotation.is_none() {
                self.check_type_for_missing_names(index_sig.type_annotation);
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Check call signatures and construct signatures for parameter properties
        if node.kind == syntax_kind_ext::CALL_SIGNATURE
            || node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
        {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                if !param.type_annotation.is_none() {
                                    self.check_type_for_parameter_properties(param.type_annotation);
                                }
                                self.maybe_report_implicit_any_parameter(param, false);
                            }
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(sig.type_annotation);
            }
        }
        // Check method signatures in type literals
        else if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_parameter_properties(&params.nodes);
                    for &param_idx in &params.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx) {
                            if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                if !param.type_annotation.is_none() {
                                    self.check_type_for_parameter_properties(param.type_annotation);
                                }
                                self.maybe_report_implicit_any_parameter(param, false);
                            }
                        }
                    }
                }
                self.check_type_for_parameter_properties(sig.type_annotation);
                if self.ctx.no_implicit_any && sig.type_annotation.is_none() {
                    if let Some(name) = self.property_name_for_error(sig.name) {
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::IMPLICIT_ANY_RETURN,
                            &[&name, "any"],
                        );
                        self.error_at_node(
                            sig.name,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_RETURN,
                        );
                    }
                }
            }
        }
        // Check property signatures for implicit any (error 7008)
        else if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if !sig.type_annotation.is_none() {
                    self.check_type_for_parameter_properties(sig.type_annotation);
                }
                // Property signature without type annotation implicitly has 'any' type
                if sig.type_annotation.is_none() {
                    if let Some(member_name) = self.get_property_name(sig.name) {
                        use crate::checker::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::MEMBER_IMPLICIT_ANY,
                            &[&member_name, "any"],
                        );
                        self.error_at_node(
                            sig.name,
                            &message,
                            diagnostic_codes::IMPLICIT_ANY_MEMBER,
                        );
                    }
                }
            }
        }
        // Check accessors in type literals/interfaces - cannot have body (error 1183)
        else if node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                // Accessors in type literals and interfaces cannot have implementations
                if !accessor.body.is_none() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    // Report error on the body
                    self.error_at_node(
                        accessor.body,
                        "An implementation cannot be declared in ambient contexts.",
                        diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
                    );
                }
            }
        }
    }

    /// Check that all method/constructor overload signatures have implementations.
    /// Reports errors 2389, 2390, 2391.
    fn check_class_member_implementations(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                i += 1;
                continue;
            };

            match node.kind {
                syntax_kind_ext::CONSTRUCTOR => {
                    if let Some(ctor) = self.ctx.arena.get_constructor(node) {
                        if ctor.body.is_none() {
                            // Constructor overload signature - check for implementation
                            let has_impl = self.find_constructor_impl(members, i + 1);
                            if !has_impl {
                                self.error_at_node(
                                    member_idx,
                                    "Constructor implementation is missing.",
                                    diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_MISSING,
                                );
                            }
                        }
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        // Abstract methods don't need implementations (they're meant for derived classes)
                        let is_abstract = self.has_abstract_modifier(&method.modifiers);
                        if method.body.is_none() && !is_abstract {
                            // Method overload signature - check for implementation
                            let method_name = self.get_method_name_from_node(member_idx);
                            if let Some(name) = method_name {
                                let (has_impl, impl_name) =
                                    self.find_method_impl(members, i + 1, &name);
                                if !has_impl {
                                    self.error_at_node(
                                        member_idx,
                                        "Function implementation is missing or not immediately following the declaration.",
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_MISSING
                                    );
                                } else if let Some(actual_name) = impl_name {
                                    if actual_name != name {
                                        // Implementation has wrong name
                                        self.error_at_node(
                                            members[i + 1],
                                            &format!(
                                                "Function implementation name must be '{}'.",
                                                name
                                            ),
                                            diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Check if there's a constructor implementation after position `start`.
    fn find_constructor_impl(&self, members: &[NodeIndex], start: usize) -> bool {
        for i in start..members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(node) {
                    if !ctor.body.is_none() {
                        return true;
                    }
                    // Another constructor overload - keep looking
                }
            } else {
                // Non-constructor member - no implementation found
                return false;
            }
        }
        false
    }

    /// Check if there's a method implementation with the given name after position `start`.
    fn find_method_impl(
        &self,
        members: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>) {
        if start >= members.len() {
            return (false, None);
        }

        let member_idx = members[start];
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return (false, None);
        };

        if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            if let Some(method) = self.ctx.arena.get_method_decl(node) {
                if !method.body.is_none() {
                    // This is an implementation - check if name matches
                    let impl_name = self.get_method_name_from_node(member_idx);
                    if let Some(ref impl_name_str) = impl_name {
                        return (true, impl_name);
                    }
                }
            }
        }
        (false, None)
    }

    /// Check that accessor pairs (get/set) have consistent abstract modifiers.
    /// Reports error TS2676 if one is abstract and the other is not.
    fn check_accessor_abstract_consistency(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Collect getters and setters by name
        #[derive(Default)]
        struct AccessorPair {
            getter: Option<(NodeIndex, bool)>, // (node_idx, is_abstract)
            setter: Option<(NodeIndex, bool)>,
        }

        let mut accessors: HashMap<String, AccessorPair> = HashMap::new();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    let is_abstract = self.has_abstract_modifier(&accessor.modifiers);

                    // Get accessor name
                    if let Some(name) = self.get_property_name(accessor.name) {
                        let pair = accessors.entry(name).or_default();
                        if node.kind == syntax_kind_ext::GET_ACCESSOR {
                            pair.getter = Some((member_idx, is_abstract));
                        } else {
                            pair.setter = Some((member_idx, is_abstract));
                        }
                    }
                }
            }
        }

        // Check for abstract mismatch
        for (_, pair) in accessors {
            if let (Some((getter_idx, getter_abstract)), Some((setter_idx, setter_abstract))) =
                (pair.getter, pair.setter)
            {
                if getter_abstract != setter_abstract {
                    // Report error on both accessors
                    self.error_at_node(
                        getter_idx,
                        "Accessors must both be abstract or non-abstract.",
                        diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
                    );
                    self.error_at_node(
                        setter_idx,
                        "Accessors must both be abstract or non-abstract.",
                        diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT,
                    );
                }
            }
        }
    }

    /// Check that accessor pairs (get/set) have compatible types.
    /// The getter return type must be assignable to the setter parameter type.
    /// Reports error TS2322 on the return statement of the getter if types mismatch.
    /// Note: Abstract accessors are skipped - they don't need type compatibility checks.
    fn check_accessor_type_compatibility(&mut self, members: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Collect getter return types and setter parameter types
        struct AccessorTypeInfo {
            getter: Option<(NodeIndex, TypeId, NodeIndex, bool, bool)>, // (accessor_idx, return_type, body_or_return_pos, is_abstract, is_declared)
            setter: Option<(NodeIndex, TypeId, bool, bool)>, // (accessor_idx, param_type, is_abstract, is_declared)
        }

        let mut accessors: HashMap<String, AccessorTypeInfo> = HashMap::new();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    if let Some(name) = self.get_property_name(accessor.name) {
                        // Check if this accessor is abstract or declared
                        let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                        let is_declared = self.has_declare_modifier(&accessor.modifiers);

                        // Get the return type - check explicit annotation first
                        let return_type = if !accessor.type_annotation.is_none() {
                            self.get_type_of_node(accessor.type_annotation)
                        } else {
                            // Infer from return statements in body
                            self.infer_getter_return_type(accessor.body)
                        };

                        // Find the position of the return statement for error reporting
                        let error_pos = self
                            .find_return_statement_pos(accessor.body)
                            .unwrap_or(member_idx);

                        let info = accessors.entry(name).or_insert_with(|| AccessorTypeInfo {
                            getter: None,
                            setter: None,
                        });
                        info.getter =
                            Some((member_idx, return_type, error_pos, is_abstract, is_declared));
                    }
                }
            } else if node.kind == syntax_kind_ext::SET_ACCESSOR {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    if let Some(name) = self.get_property_name(accessor.name) {
                        // Check if this accessor is abstract or declared
                        let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                        let is_declared = self.has_declare_modifier(&accessor.modifiers);

                        // Get the parameter type from the setter's first parameter
                        let param_type =
                            if let Some(&first_param_idx) = accessor.parameters.nodes.first() {
                                if let Some(param_node) = self.ctx.arena.get(first_param_idx) {
                                    if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                        if !param.type_annotation.is_none() {
                                            self.get_type_of_node(param.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        }
                                    } else {
                                        TypeId::ANY
                                    }
                                } else {
                                    TypeId::ANY
                                }
                            } else {
                                TypeId::ANY
                            };

                        let info = accessors.entry(name).or_insert_with(|| AccessorTypeInfo {
                            getter: None,
                            setter: None,
                        });
                        info.setter = Some((member_idx, param_type, is_abstract, is_declared));
                    }
                }
            }
        }

        // Check type compatibility for each accessor pair
        for (_, info) in accessors {
            if let (
                Some((_getter_idx, getter_type, error_pos, getter_abstract, getter_declared)),
                Some((_setter_idx, setter_type, setter_abstract, setter_declared)),
            ) = (info.getter, info.setter)
            {
                // Skip if either accessor is abstract - abstract accessors don't need type compatibility checks
                if getter_abstract || setter_abstract {
                    continue;
                }

                // Skip if either accessor is declared - declared accessors don't need type compatibility checks
                if getter_declared || setter_declared {
                    continue;
                }

                // Skip if either type is ANY (no meaningful check)
                if getter_type == TypeId::ANY || setter_type == TypeId::ANY {
                    continue;
                }

                // Check if getter return type is assignable to setter param type
                if !self.is_assignable_to(getter_type, setter_type) {
                    // Get type strings for error message
                    let getter_type_str = self.format_type(getter_type);
                    let setter_type_str = self.format_type(setter_type);

                    self.error_at_node(
                        error_pos,
                        &format!(
                            "Type '{}' is not assignable to type '{}'.",
                            getter_type_str, setter_type_str
                        ),
                        diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
            }
        }
    }

    /// Infer the return type of a getter from its body.
    fn infer_getter_return_type(&mut self, body_idx: NodeIndex) -> TypeId {
        if body_idx.is_none() {
            return TypeId::VOID;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return TypeId::VOID;
        };

        // If it's a block, look for return statements
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            if let Some(ret) = self.ctx.arena.get_return_statement(stmt_node) {
                                if !ret.expression.is_none() {
                                    return self.get_type_of_node(ret.expression);
                                }
                            }
                        }
                    }
                }
            }
        }

        // No return statements with values found - return void (not any)
        // This prevents false positive TS7010 errors for getters without return statements
        TypeId::VOID
    }

    /// Find the position of the first return statement's expression in a body.
    fn find_return_statement_pos(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }

        let body_node = self.ctx.arena.get(body_idx)?;

        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            if let Some(ret) = self.ctx.arena.get_return_statement(stmt_node) {
                                if !ret.expression.is_none() {
                                    return Some(ret.expression);
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Check that property types in derived class are compatible with base class (error 2416).
    /// For each property/accessor in the derived class, checks if there's a corresponding
    /// member in the base class with incompatible type.
    fn check_property_inheritance_compatibility(
        &mut self,
        _class_idx: NodeIndex,
        class_data: &crate::parser::thin_node::ClassData,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::scanner::SyntaxKind;
        use crate::solver::{TypeSubstitution, instantiate_type};

        // Find base class from heritage clauses (extends, not implements)
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();
        let mut base_type_argument_nodes: Option<Vec<NodeIndex>> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            if let Some(&type_idx) = heritage.types.nodes.first() {
                if let Some(type_node) = self.ctx.arena.get(type_idx) {
                    // Handle both cases:
                    // 1. ExpressionWithTypeArguments (e.g., Base<T>)
                    // 2. Simple Identifier (e.g., Base)
                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        // For simple identifiers without type arguments, the type_node itself is the identifier
                        (type_idx, None)
                    };
                    if let Some(args) = type_arguments {
                        base_type_argument_nodes = Some(args.nodes.clone());
                    }

                    // Get the class name from the expression (identifier)
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                            base_class_name = ident.escaped_text.clone();

                            // Find the base class declaration via symbol lookup
                            if let Some(sym_id) = self.ctx.binder.file_locals.get(&base_class_name)
                            {
                                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                                    // Try value_declaration first, then declarations
                                    if !symbol.value_declaration.is_none() {
                                        base_class_idx = Some(symbol.value_declaration);
                                    } else if let Some(&decl_idx) = symbol.declarations.first() {
                                        base_class_idx = Some(decl_idx);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            break; // Only one extends clause is valid
        }

        // If no base class found, nothing to check
        let Some(base_idx) = base_class_idx else {
            return;
        };

        // Get the base class data
        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return;
        };

        let mut type_args = Vec::new();
        if let Some(nodes) = base_type_argument_nodes {
            for arg_idx in nodes {
                type_args.push(self.get_type_from_type_node(arg_idx));
            }
        }

        let (base_type_params, base_type_param_updates) =
            self.push_type_parameters(&base_class.type_parameters);
        if type_args.len() < base_type_params.len() {
            for param in base_type_params.iter().skip(type_args.len()) {
                let fallback = param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                type_args.push(fallback);
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }
        let substitution = TypeSubstitution::from_args(&base_type_params, &type_args);

        // Get the derived class name for the error message
        let derived_class_name = if !class_data.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        // Check each member in the derived class
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Get the member name and type
            let (member_name, member_type, member_name_idx) = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(prop.name) else {
                        continue;
                    };

                    // Skip static properties
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }

                    // Get the type: either from annotation or inferred from initializer
                    let prop_type = if !prop.type_annotation.is_none() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if !prop.initializer.is_none() {
                        self.get_type_of_node(prop.initializer)
                    } else {
                        TypeId::ANY
                    };

                    (name, prop_type, prop.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(accessor.name) else {
                        continue;
                    };

                    // Skip static accessors
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }

                    // Get the return type
                    let accessor_type = if !accessor.type_annotation.is_none() {
                        self.get_type_from_type_node(accessor.type_annotation)
                    } else {
                        self.infer_getter_return_type(accessor.body)
                    };

                    (name, accessor_type, accessor.name)
                }
                _ => continue,
            };

            // Skip if type is ANY (no meaningful check)
            if member_type == TypeId::ANY {
                continue;
            }

            // Look for a matching member in the base class
            for &base_member_idx in &base_class.members.nodes {
                let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                    continue;
                };

                let (base_name, base_type) = match base_member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(base_prop) = self.ctx.arena.get_property_decl(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_prop.name) else {
                            continue;
                        };

                        // Skip static properties
                        if self.has_static_modifier(&base_prop.modifiers) {
                            continue;
                        }

                        let prop_type = if !base_prop.type_annotation.is_none() {
                            self.get_type_from_type_node(base_prop.type_annotation)
                        } else if !base_prop.initializer.is_none() {
                            self.get_type_of_node(base_prop.initializer)
                        } else {
                            TypeId::ANY
                        };

                        (name, prop_type)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                        let Some(base_accessor) = self.ctx.arena.get_accessor(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_accessor.name) else {
                            continue;
                        };

                        // Skip static accessors
                        if self.has_static_modifier(&base_accessor.modifiers) {
                            continue;
                        }

                        let accessor_type = if !base_accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(base_accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(base_accessor.body)
                        };

                        (name, accessor_type)
                    }
                    _ => continue,
                };

                let base_type = instantiate_type(self.ctx.types, base_type, &substitution);

                // Skip if base type is ANY
                if base_type == TypeId::ANY {
                    continue;
                }

                // Check if names match
                if member_name != base_name {
                    continue;
                }

                // Resolve TypeQuery types (typeof) before comparison
                // If member_type is `typeof y` and base_type is `typeof x`,
                // we need to compare the actual types of y and x
                let resolved_member_type = self.resolve_type_query_to_structural(member_type);
                let resolved_base_type = self.resolve_type_query_to_structural(base_type);

                // Check type compatibility - derived type must be assignable to base type
                if !self.is_assignable_to(resolved_member_type, resolved_base_type) {
                    // Format type strings for error message
                    let member_type_str = self.format_type(member_type);
                    let base_type_str = self.format_type(base_type);

                    // Report error 2416 on the member name
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{}' in type '{}' is not assignable to the same property in base type '{}'.",
                            member_name, derived_class_name, base_class_name
                        ),
                        diagnostic_codes::PROPERTY_NOT_ASSIGNABLE_TO_SAME_IN_BASE,
                    );

                    // Add secondary error with type details
                    if let Some((pos, end)) = self.get_node_span(member_name_idx) {
                        self.error(
                            pos,
                            end - pos,
                            format!(
                                "Type '{}' is not assignable to type '{}'.",
                                member_type_str, base_type_str
                            ),
                            diagnostic_codes::PROPERTY_NOT_ASSIGNABLE_TO_SAME_IN_BASE,
                        );
                    }
                }

                break; // Found matching base member, no need to continue
            }
        }

        self.pop_type_parameters(base_type_param_updates);
    }

    /// Check that interface correctly extends its base interfaces (error 2430).
    /// For each member in the derived interface, checks if the same member in a base interface
    /// has an incompatible type.
    fn check_interface_extension_compatibility(
        &mut self,
        _iface_idx: NodeIndex,
        iface_data: &crate::parser::thin_node::InterfaceData,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::scanner::SyntaxKind;
        use crate::solver::{TypeSubstitution, instantiate_type};

        // Get heritage clauses (extends)
        let Some(ref heritage_clauses) = iface_data.heritage_clauses else {
            return;
        };

        // Get the derived interface name for the error message
        let derived_name = if !iface_data.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(iface_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        let mut derived_members = Vec::new();
        for &member_idx in &iface_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind != METHOD_SIGNATURE && member_node.kind != PROPERTY_SIGNATURE {
                continue;
            }

            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            let Some(name) = self.get_property_name(sig.name) else {
                continue;
            };
            let type_id = self.get_type_of_interface_member(member_idx);
            derived_members.push((name, type_id));
        }

        // Process each heritage clause (extends)
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Process each extended interface
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };

                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    continue;
                };

                let base_name = self
                    .heritage_name_text(expr_idx)
                    .unwrap_or_else(|| base_symbol.escaped_name.clone());

                let mut base_iface_indices = Vec::new();
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_interface(node).is_some() {
                            base_iface_indices.push(decl_idx);
                        }
                    }
                }
                if base_iface_indices.is_empty() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx) {
                        if self.ctx.arena.get_interface(node).is_some() {
                            base_iface_indices.push(decl_idx);
                        }
                    }
                }

                let Some(&base_root_idx) = base_iface_indices.first() else {
                    continue;
                };

                let Some(base_root_node) = self.ctx.arena.get(base_root_idx) else {
                    continue;
                };

                let Some(base_root_iface) = self.ctx.arena.get_interface(base_root_node) else {
                    continue;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_root_iface.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param.default.or(param.constraint).unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let substitution = TypeSubstitution::from_args(&base_type_params, &type_args);

                for (member_name, member_type) in &derived_members {
                    let mut found = false;

                    for &base_iface_idx in &base_iface_indices {
                        let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                            continue;
                        };
                        let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                            continue;
                        };

                        for &base_member_idx in &base_iface.members.nodes {
                            let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                                continue;
                            };

                            let (base_member_name, base_type) = if base_member_node.kind
                                == METHOD_SIGNATURE
                                || base_member_node.kind == PROPERTY_SIGNATURE
                            {
                                if let Some(sig) = self.ctx.arena.get_signature(base_member_node) {
                                    if let Some(name) = self.get_property_name(sig.name) {
                                        let type_id =
                                            self.get_type_of_interface_member(base_member_idx);
                                        (name, type_id)
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            };

                            if *member_name != base_member_name {
                                continue;
                            }

                            found = true;
                            let base_type =
                                instantiate_type(self.ctx.types, base_type, &substitution);

                            if !self.is_assignable_to(*member_type, base_type) {
                                let member_type_str = self.format_type(*member_type);
                                let base_type_str = self.format_type(base_type);

                                self.error_at_node(
                                    iface_data.name,
                                    &format!(
                                        "Interface '{}' incorrectly extends interface '{}'.",
                                        derived_name, base_name
                                    ),
                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                );

                                if let Some((pos, end)) = self.get_node_span(iface_data.name) {
                                    self.error(
                                        pos,
                                        end - pos,
                                        format!(
                                            "Types of property '{}' are incompatible.",
                                            member_name
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                    self.error(
                                        pos,
                                        end - pos,
                                        format!(
                                            "Type '{}' is not assignable to type '{}'.",
                                            member_type_str, base_type_str
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                }

                                self.pop_type_parameters(base_type_param_updates);
                                return;
                            }

                            break;
                        }

                        if found {
                            break;
                        }
                    }
                }

                self.pop_type_parameters(base_type_param_updates);
            }
        }
    }

    /// Get the type of an interface member (method signature or property signature).
    fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::{FunctionShape, PropertyInfo};

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR; // Missing signature data - propagate error
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR; // Missing property name - propagate error
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) =
                    self.return_type_and_predicate(sig.type_annotation);

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = self.ctx.types.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                };
                return self.ctx.types.object(vec![prop]);
            }

            let type_id = if !sig.type_annotation.is_none() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
            };
            return self.ctx.types.object(vec![prop]);
        }

        TypeId::ANY
    }

    /// Check that non-abstract class implements all abstract members from base class (error 2654).
    /// Reports "Non-abstract class 'X' is missing implementations for the following members of 'Y': {members}."
    fn check_abstract_member_implementations(
        &mut self,
        class_idx: NodeIndex,
        class_data: &crate::parser::thin_node::ClassData,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::scanner::SyntaxKind;

        // Only check non-abstract classes
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        // Find base class from heritage clauses
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the base class
            if let Some(&type_idx) = heritage.types.nodes.first() {
                if let Some(type_node) = self.ctx.arena.get(type_idx) {
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                    if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                            base_class_name = ident.escaped_text.clone();

                            if let Some(sym_id) = self.ctx.binder.file_locals.get(&base_class_name)
                            {
                                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                                    if !symbol.value_declaration.is_none() {
                                        base_class_idx = Some(symbol.value_declaration);
                                    } else if let Some(&decl_idx) = symbol.declarations.first() {
                                        base_class_idx = Some(decl_idx);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            break;
        }

        let Some(base_idx) = base_class_idx else {
            return;
        };

        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return;
        };

        // Collect implemented members from derived class
        let mut implemented_members = std::collections::HashSet::new();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                // Check if this member is not abstract (i.e., it's an implementation)
                if !self.member_is_abstract(member_idx) {
                    implemented_members.insert(name);
                }
            }
        }

        // Collect abstract members from base class that are not implemented
        let mut missing_members: Vec<String> = Vec::new();
        for &member_idx in &base_class.members.nodes {
            if self.member_is_abstract(member_idx) {
                if let Some(name) = self.get_member_name(member_idx) {
                    if !implemented_members.contains(&name) {
                        missing_members.push(name);
                    }
                }
            }
        }

        // Report error if there are missing implementations
        if !missing_members.is_empty() {
            let derived_class_name = if !class_data.name.is_none() {
                if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        ident.escaped_text.clone()
                    } else {
                        String::from("<anonymous>")
                    }
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            };

            // Format: "Non-abstract class 'C' is missing implementations for the following members of 'B': 'prop', 'readonlyProp', 'm', 'mismatch'."
            let missing_list = missing_members
                .iter()
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", ");

            self.error_at_node(
                class_idx,
                &format!(
                    "Non-abstract class '{}' is missing implementations for the following members of '{}': {}.",
                    derived_class_name, base_class_name, missing_list
                ),
                diagnostic_codes::NON_ABSTRACT_CLASS_MISSING_IMPLEMENTATIONS,
            );
        }
    }

    /// Check if a class member has the abstract modifier.
    fn member_is_abstract(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.has_abstract_modifier(&prop.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_abstract_modifier(&method.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.has_abstract_modifier(&accessor.modifiers)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Get the name of a class member (property, method, or accessor).
    fn get_member_name(&self, member_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return None;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.ctx.arena.get_property_decl(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.ctx.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.ctx.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        }?;

        self.get_property_name(name_idx)
    }

    /// Get the name of a method declaration.
    /// Handles both identifier names and numeric literal names.
    fn get_method_name_from_node(&self, member_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return None;
        };

        if let Some(method) = self.ctx.arena.get_method_decl(node) {
            let Some(name_node) = self.ctx.arena.get(method.name) else {
                return None;
            };
            // Try identifier first
            if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                return Some(id.escaped_text.clone());
            }
            // Try numeric literal (for methods like 0(), 1(), etc.)
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        None
    }

    /// Check that a class properly implements all interfaces from its implements clauses.
    /// Emits TS2420 when a class incorrectly implements an interface.
    fn check_implements_clauses(
        &mut self,
        class_idx: NodeIndex,
        class_data: &crate::parser::thin_node::ClassData,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Collect implemented members from the class
        let mut class_members: std::collections::HashMap<String, NodeIndex> =
            std::collections::HashMap::new();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                class_members.insert(name, member_idx);
            }
        }

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check implements clauses
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            };

            // Check each interface in the implements clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression (identifier or property access) from ExpressionWithTypeArguments
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Get the interface symbol
                if let Some(name) = self.heritage_name_text(expr_idx) {
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&name) {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            let interface_idx = if !symbol.value_declaration.is_none() {
                                symbol.value_declaration
                            } else if let Some(&decl_idx) = symbol.declarations.first() {
                                decl_idx
                            } else {
                                continue;
                            };

                            let Some(interface_node) = self.ctx.arena.get(interface_idx) else {
                                continue;
                            };

                            // Check if it's actually an interface declaration
                            if interface_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                                continue;
                            }

                            let Some(interface_decl) = self.ctx.arena.get_interface(interface_node) else {
                                continue;
                            };

                            // Check that all interface members are implemented
                            let mut missing_members: Vec<String> = Vec::new();

                            for &member_idx in &interface_decl.members.nodes {
                                if let Some(member_name) = self.get_member_name(member_idx) {
                                    // Check if class has this member
                                    if !class_members.contains_key(&member_name) {
                                        missing_members.push(member_name);
                                    }
                                }
                            }

                            // Report error if there are missing implementations
                            if !missing_members.is_empty() {
                                let class_name = if !class_data.name.is_none() {
                                    if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                                            ident.escaped_text.clone()
                                        } else {
                                            String::from("<anonymous>")
                                        }
                                    } else {
                                        String::from("<anonymous>")
                                    }
                                } else {
                                    String::from("<anonymous>")
                                };

                                let missing_list = missing_members
                                    .iter()
                                    .map(|s| format!("'{}", s))
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                self.error_at_node(
                                    clause_idx,
                                    &format!(
                                        "Class '{}' incorrectly implements interface '{}'. Missing members: {}.",
                                        class_name, name, missing_list
                                    ),
                                    diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check that all top-level function overload signatures have implementations.
    /// Reports errors 2389, 2391.
    fn check_function_implementations(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                i += 1;
                continue;
            };

            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                if let Some(func) = self.ctx.arena.get_function(node) {
                    if func.body.is_none() {
                        if self.has_declare_modifier(&func.modifiers) {
                            i += 1;
                            continue;
                        }
                        // Function overload signature - check for implementation
                        let func_name = self.get_function_name_from_node(stmt_idx);
                        if let Some(name) = func_name {
                            let (has_impl, impl_name) =
                                self.find_function_impl(statements, i + 1, &name);
                            if !has_impl {
                                self.error_at_node(
                                    stmt_idx,
                                    "Function implementation is missing or not immediately following the declaration.",
                                    diagnostic_codes::FUNCTION_IMPLEMENTATION_MISSING
                                );
                            } else if let Some(actual_name) = impl_name {
                                if actual_name != name {
                                    // Implementation has wrong name
                                    self.error_at_node(
                                        statements[i + 1],
                                        &format!(
                                            "Function implementation name must be '{}'.",
                                            name
                                        ),
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// Check if there's a function implementation with the given name after position `start`.
    fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>) {
        if start >= statements.len() {
            return (false, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.ctx.arena.get_function(node) {
                // Check if this is an implementation (has body)
                if !func.body.is_none() {
                    // This is an implementation - check if name matches
                    let impl_name = self.get_function_name_from_node(stmt_idx);
                    return (true, impl_name);
                } else {
                    // Another overload signature without body - need to look further
                    // but we should check if this is the same function name
                    let overload_name = self.get_function_name_from_node(stmt_idx);
                    if overload_name.as_ref() == Some(&name.to_string()) {
                        // Same function, continue looking for implementation
                        return self.find_function_impl(statements, start + 1, name);
                    }
                }
            }
        }

        (false, None)
    }

    /// Get the name of a function declaration.
    fn get_function_name_from_node(&self, stmt_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return None;
        };

        if let Some(func) = self.ctx.arena.get_function(node) {
            if !func.name.is_none() {
                let Some(name_node) = self.ctx.arena.get(func.name) else {
                    return None;
                };
                if let Some(id) = self.ctx.arena.get_identifier(name_node) {
                    return Some(id.escaped_text.clone());
                }
            }
        }
        None
    }

    /// Check a module body for function overload implementations.
    fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        // Module body can be a MODULE_BLOCK or another MODULE_DECLARATION (for nested namespaces)
        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node) {
                if let Some(ref statements) = block.statements {
                    // Check statements
                    for &stmt_idx in &statements.nodes {
                        self.check_statement(stmt_idx);
                    }
                    // Check for function overload implementations
                    self.check_function_implementations(&statements.nodes);
                }
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested namespace - recurse
            self.check_statement(body_idx);
        }
    }

    /// Check for export assignment with other exported elements (error 2309).
    /// `export = X` cannot be used when there are also `export class/function/etc.`
    /// Also checks that the exported expression exists (error 2304).
    fn check_export_assignment(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let mut export_assignment_idx: Option<NodeIndex> = None;
        let mut has_other_exports = false;

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    export_assignment_idx = Some(stmt_idx);

                    // Check that the exported expression exists
                    if let Some(export_data) = self.ctx.arena.get_export_assignment(node) {
                        // Get the type of the expression (this will report 2304 if not found)
                        self.get_type_of_node(export_data.expression);
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    // export { ... } or export * from '...'
                    has_other_exports = true;
                }
                _ => {
                    // Check for export modifiers on declarations
                    // (export class X, export function f, export const x, etc.)
                    if self.has_export_modifier(stmt_idx) {
                        has_other_exports = true;
                    }
                }
            }
        }

        // Report error 2309 if there's an export assignment AND other exports
        if let Some(export_idx) = export_assignment_idx {
            if has_other_exports {
                self.error_at_node(
                    export_idx,
                    "An export assignment cannot be used in a module with other exported elements.",
                    diagnostic_codes::EXPORT_ASSIGNMENT_WITH_OTHER_EXPORTS,
                );
            }
        }
    }

    /// Check if a statement has an export modifier.
    fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        // Check different declaration types for export modifier
        let modifiers = match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => self
                .ctx
                .arena
                .get_class(node)
                .and_then(|c| c.modifiers.as_ref()),
            syntax_kind_ext::VARIABLE_STATEMENT => self
                .ctx
                .arena
                .get_variable(node)
                .and_then(|v| v.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => self
                .ctx
                .arena
                .get_interface(node)
                .and_then(|i| i.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .ctx
                .arena
                .get_type_alias(node)
                .and_then(|t| t.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => self
                .ctx
                .arena
                .get_enum(node)
                .and_then(|e| e.modifiers.as_ref()),
            _ => None,
        };

        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check that parameters don't have property modifiers (error 2369).
    /// Parameter properties (public/private/protected/readonly on params) are only
    /// allowed in constructor implementations.
    fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // If the parameter has modifiers, it's a parameter property
            // which is only allowed in constructors
            if param.modifiers.is_some() {
                self.error_at_node(
                    param_idx,
                    "A parameter property is only allowed in a constructor implementation.",
                    diagnostic_codes::PARAMETER_PROPERTY_NOT_ALLOWED,
                );
            }
        }
    }

    /// Check that parameter default values (initializers) are assignable to declared parameter types.
    /// This emits TS2322 when the default value type doesn't match the parameter type annotation.
    /// Also checks for undefined identifiers in default expressions (TS2304) regardless of type annotations.
    fn check_parameter_initializers(&mut self, parameters: &[NodeIndex]) {
        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for TS7006 in nested function expressions within the default value
            if !param.initializer.is_none() {
                self.check_for_nested_function_ts7006(param.initializer);
            }

            // Skip if there's no initializer
            if param.initializer.is_none() {
                continue;
            }

            // IMPORTANT: Always resolve the initializer expression to check for undefined identifiers (TS2304)
            // This must happen regardless of whether there's a type annotation.
            let init_type = self.get_type_of_node(param.initializer);

            // Only check type assignability if there's a type annotation
            if param.type_annotation.is_none() {
                continue;
            }

            // Get the declared parameter type
            let declared_type = self.get_type_from_type_node(param.type_annotation);

            // Check if the initializer type is assignable to the declared type
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) && !self.is_assignable_to(init_type, declared_type) {
                self.error_type_not_assignable_with_reason_at(init_type, declared_type, param_idx);
            }
        }
    }

    /// Recursively check for TS7006 in nested function/arrow expressions within a node.
    /// This handles cases like `async function foo(a = x => x)` where the nested arrow function
    /// parameter `x` should trigger TS7006 if it lacks a type annotation.
    fn check_for_nested_function_ts7006(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Check if this is a function or arrow expression
        let is_function = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => true,
            k if k == syntax_kind_ext::ARROW_FUNCTION => true,
            _ => false,
        };

        if is_function {
            // Check all parameters of this function for TS7006
            if let Some(func) = self.ctx.arena.get_function(node) {
                for &param_idx in &func.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx) {
                        if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                            // Nested functions in default values don't have contextual types
                            self.maybe_report_implicit_any_parameter(param, false);
                        }
                    }
                }
            }

            // Recursively check the function body for more nested functions
            if let Some(func) = self.ctx.arena.get_function(node) {
                if !func.body.is_none() {
                    self.check_for_nested_function_ts7006(func.body);
                }
            }
        } else {
            // Recursively check child nodes for function expressions
            match node.kind {
                // Binary expressions - check both sides
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        self.check_for_nested_function_ts7006(bin_expr.left);
                        self.check_for_nested_function_ts7006(bin_expr.right);
                    }
                }
                // Conditional expressions - check condition, then/else branches
                k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                        self.check_for_nested_function_ts7006(cond.condition);
                        self.check_for_nested_function_ts7006(cond.when_true);
                        if !cond.when_false.is_none() {
                            self.check_for_nested_function_ts7006(cond.when_false);
                        }
                    }
                }
                // Call expressions - check arguments
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.ctx.arena.get_call_expr(node) {
                        self.check_for_nested_function_ts7006(call.expression);
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.check_for_nested_function_ts7006(arg);
                            }
                        }
                    }
                }
                // Parenthesized expression - check contents
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                        self.check_for_nested_function_ts7006(paren.expression);
                    }
                }
                // Type assertion - check expression
                k if k == syntax_kind_ext::TYPE_ASSERTION => {
                    if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                        self.check_for_nested_function_ts7006(assertion.expression);
                    }
                }
                // Spread element - check expression
                k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                    if let Some(spread) = self.ctx.arena.get_spread(node) {
                        self.check_for_nested_function_ts7006(spread.expression);
                    }
                }
                _ => {
                    // For other node types, we don't recursively check
                    // This covers literals, identifiers, array/object literals, etc.
                }
            }
        }
    }

    fn node_text(&self, node_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(node_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_str();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(source[start..end].to_string())
    }

    fn parameter_name_for_error(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return "this".to_string();
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }

        self.node_text(name_idx)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "parameter".to_string())
    }

    fn property_name_for_error(&self, name_idx: NodeIndex) -> Option<String> {
        self.get_property_name(name_idx).or_else(|| {
            self.node_text(name_idx)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        })
    }

    fn is_this_parameter_name(&self, name_idx: NodeIndex) -> bool {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text == "this";
            }
        }
        false
    }

    fn maybe_report_implicit_any_parameter(
        &mut self,
        param: &crate::parser::thin_node::ParameterData,
        has_contextual_type: bool,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.no_implicit_any || has_contextual_type {
            return;
        }
        // Skip parameters that have explicit type annotations
        if !param.type_annotation.is_none() {
            return;
        }
        // Skip parameters with default values - TypeScript infers the type from the initializer
        if !param.initializer.is_none() {
            return;
        }
        if self.is_this_parameter_name(param.name) {
            return;
        }

        // Skip destructuring parameters (object/array binding patterns)
        // TypeScript doesn't emit TS7006 for destructuring parameters
        if let Some(name_node) = self.ctx.arena.get(param.name) {
            let kind = name_node.kind;
            if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                return;
            }
        }

        let param_name = self.parameter_name_for_error(param.name);
        // Rest parameters implicitly have 'any[]' type, regular parameters have 'any'
        let implicit_type = if param.dot_dot_dot_token {
            "any[]"
        } else {
            "any"
        };
        let message = format_message(
            diagnostic_messages::PARAMETER_IMPLICIT_ANY,
            &[&param_name, implicit_type],
        );
        self.error_at_node(
            param.name,
            &message,
            diagnostic_codes::IMPLICIT_ANY_PARAMETER,
        );
    }

    /// Report an error at a specific node.
    fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message.to_string(),
                category: DiagnosticCategory::Error,
                code,
                related_information: Vec::new(),
            });
        }
    }

    /// Report an error at a specific position.
    fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic {
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: message.to_string(),
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        });
    }

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }

    /// Check an expression node for TS1359: await outside async function.
    /// Recursively checks the expression tree for await expressions.
    fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        // If this is an await expression, check if we're in async context
        if node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            if !self.ctx.in_async_context() {
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages,
                };
                self.error_at_node(
                    expr_idx,
                    diagnostic_messages::AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION,
                    diagnostic_codes::AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION,
                );
            }
        }

        // Recursively check child expressions
        match node.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                    self.check_await_expression(bin_expr.left);
                    self.check_await_expression(bin_expr.right);
                }
            }
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_await_expression(unary_expr.expression);
                }
            }
            syntax_kind_ext::AWAIT_EXPRESSION => {
                // Already checked above
                if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.check_await_expression(unary_expr.expression);
                }
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call_expr) = self.ctx.arena.get_call_expr(node) {
                    self.check_await_expression(call_expr.expression);
                    // Check arguments
                    if let Some(ref args) = call_expr.arguments {
                        for &arg in &args.nodes {
                            self.check_await_expression(arg);
                        }
                    }
                }
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.ctx.arena.get_access_expr(node) {
                    self.check_await_expression(access_expr.expression);
                }
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                // Element access is stored differently - need to check the actual structure
                // The expression and argument are stored in specific data_index positions
                // For now, skip this to avoid breaking the build
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren_expr) = self.ctx.arena.get_parenthesized(node) {
                    self.check_await_expression(paren_expr.expression);
                }
            }
            _ => {
                // For other expression types, don't recurse into children
                // to avoid infinite recursion or performance issues
            }
        }
    }

    /// Report an error with context about a related symbol.

    fn class_member_is_static(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .map(|prop| self.has_static_modifier(&prop.modifiers))
                .unwrap_or(false),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map(|method| self.has_static_modifier(&method.modifiers))
                .unwrap_or(false),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .map(|accessor| self.has_static_modifier(&accessor.modifiers))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Extract the private brand property name from a type if it has one.
    /// Returns `Some(brand_name)` if the type has a private brand, `None` otherwise.
    fn get_private_brand(&self, type_id: TypeId) -> Option<String> {
        use crate::solver::TypeKey;

        let key = self.ctx.types.lookup(type_id)?;
        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    if name.starts_with("__private_brand_") {
                        return Some(name);
                    }
                }
                None
            }
            TypeKey::Callable(callable_id) => {
                // Constructor types (Callable) can also have private brands for static members
                let callable = self.ctx.types.callable_shape(callable_id);
                for prop in &callable.properties {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    if name.starts_with("__private_brand_") {
                        return Some(name);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Check if two types have the same private brand (i.e., are from the same class declaration).
    /// This is used for nominal typing of private member access.
    fn types_have_same_private_brand(&self, type1: TypeId, type2: TypeId) -> bool {
        match (self.get_private_brand(type1), self.get_private_brand(type2)) {
            (Some(brand1), Some(brand2)) => brand1 == brand2,
            _ => false,
        }
    }

    /// Extract the name of the private field from a brand string.
    /// Returns the private field name (e.g., "#foo") if found, None otherwise.
    fn get_private_field_name_from_brand(&self, type_id: TypeId) -> Option<String> {
        use crate::solver::TypeKey;

        let key = self.ctx.types.lookup(type_id)?;
        let properties = match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                &self.ctx.types.object_shape(shape_id).properties
            }
            TypeKey::Callable(callable_id) => {
                &self.ctx.types.callable_shape(callable_id).properties
            }
            _ => return None,
        };

        // Find the first non-brand private property (starts with #)
        for prop in properties {
            let name = self.ctx.types.resolve_atom(prop.name);
            if name.starts_with('#') && !name.starts_with("__private_brand_") {
                return Some(name);
            }
        }
        None
    }

    /// Check if there's a private brand mismatch between two types and return an appropriate error message.
    /// Returns Some(error_message) if there's a private brand mismatch, None otherwise.
    fn private_brand_mismatch_error(&self, source: TypeId, target: TypeId) -> Option<String> {
        let source_brand = self.get_private_brand(source)?;
        let target_brand = self.get_private_brand(target)?;

        // Only report if both have brands but they're different
        if source_brand == target_brand {
            return None;
        }

        // Try to get the private field name from the source type
        let field_name = self.get_private_field_name_from_brand(source)
            .unwrap_or_else(|| "[private field]".to_string());

        Some(format!(
            "Property '{}' in type '{}' refers to a different member that cannot be accessed from within type '{}'.",
            field_name,
            self.format_type(source),
            self.format_type(target)
        ))
    }

    fn private_member_declaring_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if !matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
            ) {
                continue;
            }

            let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                continue;
            };
            if ext.parent.is_none() {
                continue;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                continue;
            };
            if parent_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && parent_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(parent_node) else {
                continue;
            };
            let is_static = self.class_member_is_static(decl_idx);
            return Some(if is_static {
                self.get_class_constructor_type(ext.parent, class)
            } else {
                self.get_class_instance_type(ext.parent, class)
            });
        }

        None
    }

    fn class_member_this_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let class_idx = class_info.class_idx;
        let is_static = self.class_member_is_static(member_idx);

        if !is_static {
            // Use the current class type parameters in scope for instance `this`.
            if let Some(node) = self.ctx.arena.get(class_idx) {
                if let Some(class) = self.ctx.arena.get_class(node) {
                    return Some(self.get_class_instance_type(class_idx, class));
                }
            }
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx) {
            if is_static {
                return Some(self.get_type_of_symbol(sym_id));
            }
            return self.class_instance_type_from_symbol(sym_id);
        }

        let node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(node)?;
        Some(if is_static {
            self.get_class_constructor_type(class_idx, class)
        } else {
            self.get_class_instance_type(class_idx, class)
        })
    }

    fn check_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        let _ = self.get_type_of_node(computed.expression);
    }

    fn check_class_member_name(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.check_computed_property_name(prop.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.check_computed_property_name(method.name);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.check_computed_property_name(accessor.name);
                }
            }
            _ => {}
        }
    }

    /// Check a class member (property, method, constructor, accessor).
    fn check_class_member(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let mut pushed_this = false;
        if let Some(this_type) = self.class_member_this_type(member_idx) {
            self.ctx.this_type_stack.push(this_type);
            pushed_this = true;
        }

        self.check_class_member_name(member_idx);

        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.check_property_declaration(member_idx);
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.check_method_declaration(member_idx);
            }
            syntax_kind_ext::CONSTRUCTOR => {
                self.check_constructor_declaration(member_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.check_accessor_declaration(member_idx);
            }
            _ => {
                // Other class member types (static blocks, index signatures, etc.)
                self.get_type_of_node(member_idx);
            }
        }

        if pushed_this {
            self.ctx.this_type_stack.pop();
        }
    }

    /// Check for TS2729: Property is used before its initialization.
    /// This checks if a property initializer references another property via `this.X`
    /// where X is declared after the current property.
    fn check_property_initialization_order(
        &mut self,
        current_prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Get class info to access member order
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return;
        };

        // Find the position of the current property in the member list
        let Some(current_pos) = class_info
            .member_nodes
            .iter()
            .position(|&idx| idx == current_prop_idx)
        else {
            return;
        };

        // Collect all `this.X` property accesses in the initializer
        let accesses = self.collect_this_property_accesses(initializer_idx);

        for (name, access_node_idx) in accesses {
            // Find if this name refers to another property in the class
            for (target_pos, &target_idx) in class_info.member_nodes.iter().enumerate() {
                if let Some(member_name) = self.get_member_name(target_idx) {
                    if member_name == name {
                        // Check if target is an instance property (not static, not a method)
                        if self.is_instance_property(target_idx) {
                            // Report 2729 if:
                            // 1. Target is declared after current property, OR
                            // 2. Target is an abstract property (no initializer in this class)
                            let should_error =
                                target_pos > current_pos || self.is_abstract_property(target_idx);
                            if should_error {
                                self.error_at_node(
                                    access_node_idx,
                                    &format!(
                                        "Property '{}' is used before its initialization.",
                                        name
                                    ),
                                    diagnostic_codes::PROPERTY_USED_BEFORE_INITIALIZATION,
                                );
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    /// Check if a property declaration is abstract (has abstract modifier).
    fn is_abstract_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            return self.has_abstract_modifier(&prop.modifiers);
        }

        false
    }

    /// Collect all `this.propertyName` accesses in an expression.
    /// Stops at function boundaries where `this` context changes.
    fn collect_this_property_accesses(&self, node_idx: NodeIndex) -> Vec<(String, NodeIndex)> {
        let mut accesses = Vec::new();
        self.collect_this_accesses_recursive(node_idx, &mut accesses);
        accesses
    }

    /// Recursive helper to collect this.X accesses.
    /// Uses the BinaryExprData structure which is used for property access in our arena.
    fn collect_this_accesses_recursive(
        &self,
        node_idx: NodeIndex,
        accesses: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // Stop at function boundaries where `this` context changes
        // (but not arrow functions, which preserve `this`)
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return;
        }

        // Property access uses AccessExprData with expression and name_or_argument
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                // Check if the expression is `this`
                if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                    if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
                        // Get the property name
                        if let Some(name_node) = self.ctx.arena.get(access.name_or_argument) {
                            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                                accesses.push((ident.escaped_text.clone(), node_idx));
                            }
                        }
                    } else {
                        // Recurse into the expression part
                        self.collect_this_accesses_recursive(access.expression, accesses);
                    }
                }
            }
            return;
        }

        // For other nodes, recurse into children based on node type
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_this_accesses_recursive(binary.left, accesses);
                    self.collect_this_accesses_recursive(binary.right, accesses);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_this_accesses_recursive(call.expression, accesses);
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            self.collect_this_accesses_recursive(arg, accesses);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_this_accesses_recursive(paren.expression, accesses);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_this_accesses_recursive(cond.condition, accesses);
                    self.collect_this_accesses_recursive(cond.when_true, accesses);
                    self.collect_this_accesses_recursive(cond.when_false, accesses);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                // Arrow functions: while they preserve `this` context, property access
                // inside is deferred until the function is called. So we don't recurse
                // because the access doesn't happen during initialization.
                // (This matches TypeScript's behavior for error 2729)
            }
            _ => {
                // For other expressions, we don't recurse further to keep it simple
            }
        }
    }

    /// Check if a class member is an instance property (not static, not a method/accessor).
    fn is_instance_property(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return false;
        }

        if let Some(prop) = self.ctx.arena.get_property_decl(node) {
            // Check if it has a static modifier
            return !self.has_static_modifier(&prop.modifiers);
        }

        false
    }

    /// Check a property declaration.
    fn check_property_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(prop) = self.ctx.arena.get_property_decl(node) else {
            return;
        };

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&prop.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::CONST_MODIFIER_CANNOT_APPEAR_ON_A_CLASS_ELEMENT,
            );
        }

        // If property has type annotation and initializer, check type compatibility
        if !prop.type_annotation.is_none() && !prop.initializer.is_none() {
            let declared_type = self.get_type_from_type_node(prop.type_annotation);
            let prev_context = self.ctx.contextual_type;
            if declared_type != TypeId::ANY && !self.type_contains_error(declared_type) {
                self.ctx.contextual_type = Some(declared_type);
            }
            let init_type = self.get_type_of_node(prop.initializer);
            self.ctx.contextual_type = prev_context;

            if declared_type != TypeId::ANY && !self.is_assignable_to(init_type, declared_type) {
                self.error_type_not_assignable_with_reason_at(
                    init_type,
                    declared_type,
                    prop.initializer,
                );
            }
        } else if !prop.initializer.is_none() {
            // Just check the initializer to catch errors within it
            self.get_type_of_node(prop.initializer);
        }

        // Error 2729: Property is used before its initialization
        // Check if initializer references properties declared after this one
        if !prop.initializer.is_none() && !self.has_static_modifier(&prop.modifiers) {
            self.check_property_initialization_order(member_idx, prop.initializer);
        }

        // TS7008: Member implicitly has an 'any' type
        // Report this error when noImplicitAny is enabled and the property has no type annotation
        // AND no initializer (if there's an initializer, TypeScript can infer the type)
        if self.ctx.no_implicit_any && prop.type_annotation.is_none() && prop.initializer.is_none() {
            if let Some(member_name) = self.get_property_name(prop.name) {
                use crate::checker::types::diagnostics::{
                    diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::MEMBER_IMPLICIT_ANY,
                    &[&member_name, "any"],
                );
                self.error_at_node(
                    prop.name,
                    &message,
                    diagnostic_codes::IMPLICIT_ANY_MEMBER,
                );
            }
        }
    }

    /// Check a method declaration.
    fn check_method_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(method) = self.ctx.arena.get_method_decl(node) else {
            return;
        };

        // Error 1248: A class member cannot have the 'const' keyword
        if let Some(const_mod) = self.get_const_modifier(&method.modifiers) {
            self.error_at_node(
                const_mod,
                "A class member cannot have the 'const' keyword.",
                diagnostic_codes::CONST_MODIFIER_CANNOT_APPEAR_ON_A_CLASS_ELEMENT,
            );
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the method has a body
        if !method.body.is_none() {
            if let Some(ref class_info) = self.ctx.enclosing_class {
                if class_info.is_declared {
                    self.error_at_node(
                        member_idx,
                        "An implementation cannot be declared in ambient contexts.",
                        diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
                    );
                }
            }
        }

        // Push type parameters (like <U> in `fn<U>(id: U)`) before checking types
        let (_type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);

        // Extract parameter types from contextual type (for object literal methods)
        // This enables shorthand method parameter type inference
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        if let Some(ctx_type) = self.ctx.contextual_type {
            let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, ctx_type);

            for (i, &param_idx) in method.parameters.nodes.iter().enumerate() {
                if let Some(param_node) = self.ctx.arena.get(param_idx) {
                    if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                        let type_id = if !param.type_annotation.is_none() {
                            // Use explicit type annotation if present
                            Some(self.get_type_from_type_node(param.type_annotation))
                        } else {
                            // Infer from contextual type
                            ctx_helper.get_parameter_type(i)
                        };
                        param_types.push(type_id);
                    }
                }
            }
        }

        let has_type_annotation = !method.type_annotation.is_none();
        let mut return_type = if has_type_annotation {
            self.get_type_from_type_node(method.type_annotation)
        } else {
            TypeId::ANY
        };

        // Cache parameter types for use in method body
        // If we have contextual types, use them; otherwise fall back to type annotations or UNKNOWN
        if param_types.is_empty() {
            self.cache_parameter_types(&method.parameters.nodes, None);
        } else {
            self.cache_parameter_types(&method.parameters.nodes, Some(&param_types));
        }

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&method.parameters);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&method.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in methods
        self.check_parameter_properties(&method.parameters.nodes);

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &method.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx) {
                if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                    if !param.type_annotation.is_none() {
                        self.check_type_for_parameter_properties(param.type_annotation);
                    }
                    self.maybe_report_implicit_any_parameter(param, false);
                }
            }
        }

        // Check return type annotation for parameter properties in function types
        if !method.type_annotation.is_none() {
            self.check_type_for_parameter_properties(method.type_annotation);
        }

        // Check method body
        if !method.body.is_none() {
            if !has_type_annotation {
                return_type = self.infer_return_type_from_body(method.body, None);
            }

            // TS7011 (implicit any return) is only emitted for ambient methods,
            // matching TypeScript's behavior
            let is_ambient_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .map(|c| c.is_declared)
                .unwrap_or(false);
            let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");

            if is_ambient_class || is_ambient_file {
                let method_name = self.get_property_name(method.name);
                self.maybe_report_implicit_any_return(
                    method_name,
                    Some(method.name),
                    return_type,
                    has_type_annotation,
                    false,
                    member_idx,
                );
            }

            self.push_return_type(return_type);
            self.check_statement(method.body);

            let is_async = self.has_async_modifier(&method.modifiers);
            let is_generator = method.asterisk_token;
            let check_return_type =
                self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
            let requires_return = self.requires_return_value(check_return_type);
            let has_return = self.body_has_return_with_value(method.body);
            let falls_through = self.function_body_falls_through(method.body);

            // TS2355: Skip for async methods - they implicitly return Promise<void>
            if has_type_annotation && requires_return && falls_through && !is_async {
                if !has_return {
                    self.error_at_node(
                        method.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                    );
                } else {
                    use crate::checker::types::diagnostics::diagnostic_messages;
                    self.error_at_node(
                        method.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                    );
                }
            } else if self.ctx.no_implicit_returns && has_return && falls_through {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::checker::types::diagnostics::diagnostic_messages;
                let error_node = if !method.name.is_none() {
                    method.name
                } else {
                    method.body
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                );
            }

            self.pop_return_type();
        } else {
            // Abstract method or method overload signature
            // Report TS7010 for abstract methods without return type annotation
            let method_name = self.get_property_name(method.name);
            self.maybe_report_implicit_any_return(
                method_name,
                Some(method.name),
                return_type,
                has_type_annotation,
                false,
                member_idx,
            );
        }

        self.pop_type_parameters(type_param_updates);
    }

    /// Check a constructor declaration.
    fn check_constructor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(ctor) = self.ctx.arena.get_constructor(node) else {
            return;
        };

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the constructor has a body
        if !ctor.body.is_none() {
            if let Some(ref class_info) = self.ctx.enclosing_class {
                if class_info.is_declared {
                    self.error_at_node(
                        member_idx,
                        "An implementation cannot be declared in ambient contexts.",
                        diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
                    );
                }
            }
        }

        // Check for parameter properties in constructor overload signatures (error 2369)
        // Parameter properties are only allowed in constructor implementations (with body)
        if ctor.body.is_none() {
            self.check_parameter_properties(&ctor.parameters.nodes);
        }

        // Check parameter type annotations for parameter properties in function types
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx) {
                if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                    if !param.type_annotation.is_none() {
                        self.check_type_for_parameter_properties(param.type_annotation);
                    }
                    self.maybe_report_implicit_any_parameter(param, false);
                }
            }
        }

        // Constructors don't have explicit return types, but they implicitly return the class instance type
        // Get the class instance type to validate constructor return expressions (TS2322)

        self.cache_parameter_types(&ctor.parameters.nodes, None);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&ctor.parameters);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&ctor.parameters.nodes);

        // Set in_constructor flag for abstract property checks (error 2715)
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = true;
        }

        // Check constructor body
        if !ctor.body.is_none() {
            // Get class instance type for constructor return expression validation
            let instance_type = if let Some(ref class_info) = self.ctx.enclosing_class {
                let class_node = self.ctx.arena.get(class_info.class_idx);
                if let Some(class) = class_node.and_then(|n| self.ctx.arena.get_class(n)) {
                    self.get_class_instance_type(class_info.class_idx, class)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };

            // Set expected return type to class instance type
            self.push_return_type(instance_type);
            self.check_statement(ctor.body);
            self.pop_return_type();
        }

        // Reset in_constructor flag
        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_constructor = false;
        }
    }

    /// Check an accessor declaration (getter/setter).
    fn check_accessor_declaration(&mut self, member_idx: NodeIndex) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let Some(accessor) = self.ctx.arena.get_accessor(node) else {
            return;
        };

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if we're in a declared class and the accessor has a body
        if !accessor.body.is_none() {
            if let Some(ref class_info) = self.ctx.enclosing_class {
                if class_info.is_declared {
                    self.error_at_node(
                        member_idx,
                        "An implementation cannot be declared in ambient contexts.",
                        diagnostic_codes::IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT,
                    );
                }
            }
        }

        let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;
        let has_type_annotation = is_getter && !accessor.type_annotation.is_none();
        let mut return_type = if is_getter {
            if has_type_annotation {
                self.get_type_from_type_node(accessor.type_annotation)
            } else {
                TypeId::VOID // Default to void for getters without type annotation
            }
        } else {
            TypeId::VOID
        };

        self.cache_parameter_types(&accessor.parameters.nodes, None);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&accessor.parameters.nodes);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in accessors
        self.check_parameter_properties(&accessor.parameters.nodes);

        // TS7006 (implicit any parameter) is NOT emitted for setter parameters
        // because the setter parameter type is inferred from the getter's return type
        // Only check getter parameters
        if is_getter {
            for &param_idx in &accessor.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx) {
                    if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                        self.maybe_report_implicit_any_parameter(param, false);
                    }
                }
            }
        }

        // For setters, check parameter constraints (1052, 1053)
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.check_setter_parameter(&accessor.parameters.nodes);
        }

        // Check accessor body
        if !accessor.body.is_none() {
            if is_getter && !has_type_annotation {
                return_type = self.infer_getter_return_type(accessor.body);
            }

            // TS7010 (implicit any return) is only emitted for ambient accessors,
            // matching TypeScript's behavior
            if is_getter {
                let is_ambient_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .map(|c| c.is_declared)
                    .unwrap_or(false);
                let is_ambient_file = self.ctx.file_name.ends_with(".d.ts");

                if is_ambient_class || is_ambient_file {
                    let accessor_name = self.get_property_name(accessor.name);
                    self.maybe_report_implicit_any_return(
                        accessor_name,
                        Some(accessor.name),
                        return_type,
                        has_type_annotation,
                        false,
                        member_idx,
                    );
                }
            }

            self.push_return_type(return_type);
            self.check_statement(accessor.body);
            if is_getter {
                // Check if this is an async getter
                let is_async = self.has_async_modifier(&accessor.modifiers);
                let requires_return = self.requires_return_value(return_type);
                let has_return = self.body_has_return_with_value(accessor.body);
                let falls_through = self.function_body_falls_through(accessor.body);
                // TS2355: Skip for async getters - they implicitly return Promise<void>
                if has_type_annotation && requires_return && falls_through && !is_async {
                    if !has_return {
                        self.error_at_node(
                            accessor.type_annotation,
                            "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                            diagnostic_codes::FUNCTION_LACKS_RETURN_TYPE,
                        );
                    } else {
                        use crate::checker::types::diagnostics::diagnostic_messages;
                        self.error_at_node(
                            accessor.type_annotation,
                            diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT,
                            diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_VALUE,
                        );
                    }
                } else if self.ctx.no_implicit_returns && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::checker::types::diagnostics::diagnostic_messages;
                    let error_node = if !accessor.name.is_none() {
                        accessor.name
                    } else {
                        accessor.body
                    };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN,
                    );
                }
            }
            self.pop_return_type();
        }
    }

    /// Check setter parameter constraints (1052, 1053).
    /// - A 'set' accessor parameter cannot have an initializer
    /// - A 'set' accessor cannot have rest parameter
    fn check_setter_parameter(&mut self, parameters: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for initializer (error 1052)
            if !param.initializer.is_none() {
                self.error_at_node(
                    param.name,
                    "A 'set' accessor parameter cannot have an initializer.",
                    diagnostic_codes::SETTER_PARAMETER_CANNOT_HAVE_INITIALIZER,
                );
            }

            // Check for rest parameter (error 1053)
            if param.dot_dot_dot_token {
                self.error_at_node(
                    param_idx,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::SETTER_CANNOT_HAVE_REST_PARAMETER,
                );
            }

            // Check for implicit any (error 7006)
            // Setter parameters without type annotation implicitly have 'any' type
            self.maybe_report_implicit_any_parameter(param, false);
        }
    }

    /// Check if a return type requires a return value.
    /// Returns false for void, undefined, any, and never.
    fn requires_return_value(&self, return_type: TypeId) -> bool {
        use crate::solver::TypeKey;

        // void, undefined, any, never don't require a return value
        if return_type == TypeId::VOID
            || return_type == TypeId::UNDEFINED
            || return_type == TypeId::ANY
            || return_type == TypeId::NEVER
            || return_type == TypeId::UNKNOWN
            || return_type == TypeId::ERROR
        {
            return false;
        }

        // Check for union types that include void/undefined
        if let Some(TypeKey::Union(members)) = self.ctx.types.lookup(return_type) {
            let members = self.ctx.types.type_list(members);
            for &member in members.iter() {
                if member == TypeId::VOID || member == TypeId::UNDEFINED {
                    return false;
                }
            }
        }

        true
    }

    fn return_type_for_implicit_return_check(
        &mut self,
        return_type: TypeId,
        is_async: bool,
        is_generator: bool,
    ) -> TypeId {
        if is_generator {
            return TypeId::UNKNOWN; // Generator support not implemented - use UNKNOWN
        }

        if is_async {
            if let Some(inner) = self.promise_like_return_type_argument(return_type) {
                return inner;
            }
        }

        return_type
    }

    fn promise_like_return_type_argument(&mut self, return_type: TypeId) -> Option<TypeId> {
        use crate::solver::{SymbolRef, TypeKey};

        if let Some(TypeKey::Application(app_id)) = self.ctx.types.lookup(return_type) {
            let app = self.ctx.types.type_application(app_id);

            // Try to get the type argument from the base symbol
            if let Some(result) =
                self.promise_like_type_argument_from_base(app.base, &app.args, &mut Vec::new())
            {
                return Some(result);
            }

            // Fallback: if the base is a Promise-like reference (e.g., Promise from lib files)
            // and we have type arguments, return the first one
            // This handles cases where Promise doesn't have expected flags or where
            // promise_like_type_argument_from_base fails for other reasons
            if let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(app.base) {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                    if self.is_promise_like_name(symbol.escaped_name.as_str()) {
                        if let Some(&first_arg) = app.args.first() {
                            return Some(first_arg);
                        }
                    }
                }
            }
        }

        // If we can't extract the type argument from a Promise-like type,
        // return None instead of ANY/UNKNOWN (consistent with Task 4-6 changes)
        // This allows the caller (await expressions) to use UNKNOWN as fallback
        None
    }

    fn type_ref_is_promise_like(&self, type_id: TypeId) -> bool {
        use crate::solver::{SymbolRef, TypeKey};

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Ref(SymbolRef(sym_id))) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                    return self.is_promise_like_name(symbol.escaped_name.as_str());
                }
            }
            Some(TypeKey::Application(app_id)) => {
                // Check if the base type of the application is a Promise-like type
                let app = self.ctx.types.type_application(app_id);
                return self.type_ref_is_promise_like(app.base);
            }
            Some(TypeKey::Object(_)) => {
                // For Object types (interfaces from lib files), we conservatively assume
                // they might be Promise-like. This avoids false positives for Promise<void>
                // return types from lib files where we can't easily determine the interface name.
                // A more precise check would require tracking the original type reference.
                return true;
            }
            _ => {}
        }
        false
    }

    fn promise_like_type_argument_from_base(
        &mut self,
        base: TypeId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        use crate::solver::{SymbolRef, TypeKey};

        let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(base) else {
            return None;
        };
        let sym_id = SymbolId(sym_id);

        // Try to get the symbol, but handle the case where it doesn't exist (e.g., import from missing module)
        let symbol = self.ctx.binder.get_symbol(sym_id);

        // If symbol doesn't exist, we can still check if we have type arguments to extract
        // This handles cases like `MyPromise<void>` where MyPromise is imported from a missing module
        if symbol.is_none() {
            // For unresolved Promise-like types, assume the inner type is the first type argument
            // This allows async functions with unresolved Promise return types to be handled gracefully
            if let Some(&first_arg) = args.first() {
                return Some(first_arg);
            }
            // Return UNKNOWN instead of ANY when there are no type arguments (consistent with Task 4-6)
            return Some(TypeId::UNKNOWN);
        }

        let symbol = symbol.unwrap();
        let name = symbol.escaped_name.as_str();

        if self.is_promise_like_name(name) {
            // Return UNKNOWN instead of ANY when there are no type arguments (consistent with Task 4-6)
            return Some(args.first().copied().unwrap_or(TypeId::UNKNOWN));
        }

        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            return self.promise_like_type_argument_from_alias(sym_id, args, visited_aliases);
        }

        if symbol.flags & symbol_flags::CLASS != 0 {
            return self.promise_like_type_argument_from_class(sym_id, args, visited_aliases);
        }

        None
    }

    fn promise_like_type_argument_from_alias(
        &mut self,
        sym_id: SymbolId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        use crate::solver::TypeKey;

        if visited_aliases.iter().any(|&seen| seen == sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let type_alias = self.ctx.arena.get_type_alias(node)?;

        let mut bindings = Vec::new();
        if let Some(params) = &type_alias.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check if the alias RHS is directly a Promise/PromiseLike type reference
        // before lowering (e.g., Promise<T> where Promise is from lib and might not fully resolve)
        if let Some(type_node) = self.ctx.arena.get(type_alias.type_node) {
            if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                if let Some(name_node) = self.ctx.arena.get(type_ref.type_name) {
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        if self.is_promise_like_name(ident.escaped_text.as_str()) {
                            // It's Promise<...> or PromiseLike<...>
                            // Get the first type argument and substitute bindings
                            if let Some(type_args) = &type_ref.type_arguments {
                                if let Some(&first_arg_idx) = type_args.nodes.first() {
                                    // Try to substitute bindings in the type argument
                                    let arg_type = self
                                        .lower_type_with_bindings(first_arg_idx, bindings.clone());
                                    return Some(arg_type);
                                }
                            }
                            // No type args means Promise (equivalent to Promise<any>)
                            return Some(TypeId::ANY);
                        }
                    }
                }
            }
        }

        let lowered = self.lower_type_with_bindings(type_alias.type_node, bindings);
        if let Some(TypeKey::Application(app_id)) = self.ctx.types.lookup(lowered) {
            let app = self.ctx.types.type_application(app_id);
            return self.promise_like_type_argument_from_base(app.base, &app.args, visited_aliases);
        }

        // Fallback: if the alias expands to a promise-like type reference (e.g., Promise from lib),
        // treat it as Promise<unknown> if we can't get the type argument.
        // This handles cases like: type PromiseAlias<T> = Promise<T> where Promise comes from lib.
        if self.type_ref_is_promise_like(lowered) {
            // If we have args, try to return the first one (the T in Promise<T>)
            // Otherwise return UNKNOWN for stricter type checking
            return Some(args.first().copied().unwrap_or(TypeId::UNKNOWN));
        }

        None
    }

    fn promise_like_type_argument_from_class(
        &mut self,
        sym_id: SymbolId,
        args: &[TypeId],
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        use crate::solver::TypeKey;

        if visited_aliases.iter().any(|&seen| seen == sym_id) {
            return None;
        }
        visited_aliases.push(sym_id);

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        if decl_idx.is_none() {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let class = self.ctx.arena.get_class(node)?;

        // Build type parameter bindings for this class
        let mut bindings = Vec::new();
        if let Some(params) = &class.type_parameters {
            if params.nodes.len() != args.len() {
                return None;
            }
            for (&param_idx, &arg) in params.nodes.iter().zip(args.iter()) {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                bindings.push((self.ctx.types.intern_string(&ident.escaped_text), arg));
            }
        } else if !args.is_empty() {
            return None;
        }

        // Check heritage clauses for extends Promise/PromiseLike
        let Some(heritage_clauses) = &class.heritage_clauses else {
            return None;
        };

        for &clause_idx in heritage_clauses.nodes.iter() {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            // Handle both cases:
            // 1. ExpressionWithTypeArguments (e.g., Promise<T>)
            // 2. Simple Identifier (e.g., Promise)
            let (expr_idx, type_arguments) =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    (
                        expr_type_args.expression,
                        expr_type_args.type_arguments.as_ref(),
                    )
                } else {
                    (type_idx, None)
                };

            // Get the base class name
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
                continue;
            };

            // Check if it's Promise or PromiseLike
            if !self.is_promise_like_name(&ident.escaped_text) {
                continue;
            }

            // If it extends Promise<X>, extract X and substitute type parameters
            if let Some(type_args) = type_arguments {
                if let Some(&first_arg_node) = type_args.nodes.first() {
                    let lowered = self.lower_type_with_bindings(first_arg_node, bindings);
                    return Some(lowered);
                }
            }

            // Promise with no type argument defaults to Promise<any>
            return Some(TypeId::ANY);
        }

        None
    }

    fn lower_type_with_bindings(
        &self,
        type_node: NodeIndex,
        bindings: Vec<(crate::interner::Atom, TypeId)>,
    ) -> TypeId {
        use crate::solver::TypeLowering;

        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(bindings);
        lowering.lower_type(type_node)
    }

    fn is_promise_like_name(&self, name: &str) -> bool {
        // Match exact Promise/PromiseLike names, or any name containing "Promise" (case-insensitive)
        // This handles types like MyPromise, CustomPromise, etc.
        matches!(name, "Promise" | "PromiseLike") || name.contains("Promise")
    }

    /// Check if a type is a Promise or Promise-like type.
    /// This is used to validate async function return types.
    fn is_promise_type(&self, type_id: TypeId) -> bool {
        use crate::solver::{SymbolRef, TypeKey};

        // Check for Promise<T> or PromiseLike<T> type application
        if let Some(TypeKey::Application(app_id)) = self.ctx.types.lookup(type_id) {
            let app = self.ctx.types.type_application(app_id);
            return self.type_ref_is_promise_like(app.base);
        }

        // Check for direct Promise or PromiseLike reference (this also handles type aliases)
        if let Some(TypeKey::Ref(SymbolRef(sym_id))) = self.ctx.types.lookup(type_id) {
            if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_id)) {
                return self.is_promise_like_name(symbol.escaped_name.as_str());
            }
        }

        false
    }

    fn is_null_or_undefined_only(&self, return_type: TypeId) -> bool {
        use crate::solver::TypeKey;

        if return_type == TypeId::NULL || return_type == TypeId::UNDEFINED {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.ctx.types.lookup(return_type) {
            let members = self.ctx.types.type_list(members);
            if members.is_empty() {
                return false;
            }
            return members
                .iter()
                .all(|&member| member == TypeId::NULL || member == TypeId::UNDEFINED);
        }

        false
    }

    fn type_contains_any(&self, type_id: TypeId) -> bool {
        let mut visited = Vec::new();
        self.type_contains_any_inner(type_id, &mut visited)
    }

    fn type_contains_any_inner(&self, type_id: TypeId, visited: &mut Vec<TypeId>) -> bool {
        use crate::solver::{TemplateSpan, TypeKey};

        if type_id == TypeId::ANY {
            return true;
        }
        if visited.contains(&type_id) {
            return false;
        }
        visited.push(type_id);

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Array(elem)) => self.type_contains_any_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => self
                .ctx
                .types
                .tuple_list(list_id)
                .iter()
                .any(|elem| self.type_contains_any_inner(elem.type_id, visited)),
            Some(TypeKey::Union(list_id)) | Some(TypeKey::Intersection(list_id)) => self
                .ctx
                .types
                .type_list(list_id)
                .iter()
                .any(|&member| self.type_contains_any_inner(member, visited)),
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_any_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(ref index) = shape.string_index {
                    if self.type_contains_any_inner(index.value_type, visited) {
                        return true;
                    }
                }
                if let Some(ref index) = shape.number_index {
                    if self.type_contains_any_inner(index.value_type, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::Function(shape_id)) => {
                let shape = self.ctx.types.function_shape(shape_id);
                self.type_contains_any_inner(shape.return_type, visited)
            }
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.type_contains_any_inner(sig.return_type, visited))
                {
                    return true;
                }
                if shape
                    .construct_signatures
                    .iter()
                    .any(|sig| self.type_contains_any_inner(sig.return_type, visited))
                {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_any_inner(prop.type_id, visited))
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.ctx.types.type_application(app_id);
                if self.type_contains_any_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|&arg| self.type_contains_any_inner(arg, visited))
            }
            Some(TypeKey::Conditional(cond_id)) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.type_contains_any_inner(cond.check_type, visited)
                    || self.type_contains_any_inner(cond.extends_type, visited)
                    || self.type_contains_any_inner(cond.true_type, visited)
                    || self.type_contains_any_inner(cond.false_type, visited)
            }
            Some(TypeKey::Mapped(mapped_id)) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                if self.type_contains_any_inner(mapped.constraint, visited) {
                    return true;
                }
                if let Some(name_type) = mapped.name_type {
                    if self.type_contains_any_inner(name_type, visited) {
                        return true;
                    }
                }
                self.type_contains_any_inner(mapped.template, visited)
            }
            Some(TypeKey::IndexAccess(base, index)) => {
                self.type_contains_any_inner(base, visited)
                    || self.type_contains_any_inner(index, visited)
            }
            Some(TypeKey::TemplateLiteral(template_id)) => self
                .ctx
                .types
                .template_list(template_id)
                .iter()
                .any(|span| match span {
                    TemplateSpan::Type(span_type) => {
                        self.type_contains_any_inner(*span_type, visited)
                    }
                    _ => false,
                }),
            Some(TypeKey::KeyOf(inner)) | Some(TypeKey::ReadonlyType(inner)) => {
                self.type_contains_any_inner(inner, visited)
            }
            Some(TypeKey::TypeParameter(info)) => {
                if let Some(constraint) = info.constraint {
                    if self.type_contains_any_inner(constraint, visited) {
                        return true;
                    }
                }
                if let Some(default) = info.default {
                    if self.type_contains_any_inner(default, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::Infer(info)) => {
                if let Some(constraint) = info.constraint {
                    if self.type_contains_any_inner(constraint, visited) {
                        return true;
                    }
                }
                if let Some(default) = info.default {
                    if self.type_contains_any_inner(default, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::TypeQuery(_))
            | Some(TypeKey::UniqueSymbol(_))
            | Some(TypeKey::ThisType)
            | Some(TypeKey::Ref(_))
            | Some(TypeKey::Literal(_))
            | Some(TypeKey::Intrinsic(_))
            | Some(TypeKey::Error)
            | None => false,
        }
    }

    fn type_contains_error(&self, type_id: TypeId) -> bool {
        let mut visited = Vec::new();
        self.type_contains_error_inner(type_id, &mut visited)
    }

    fn type_contains_error_inner(&self, type_id: TypeId, visited: &mut Vec<TypeId>) -> bool {
        use crate::solver::{TemplateSpan, TypeKey};

        if type_id == TypeId::ERROR {
            return true;
        }
        if visited.contains(&type_id) {
            return false;
        }
        visited.push(type_id);

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Array(elem)) => self.type_contains_error_inner(elem, visited),
            Some(TypeKey::Tuple(list_id)) => self
                .ctx
                .types
                .tuple_list(list_id)
                .iter()
                .any(|elem| self.type_contains_error_inner(elem.type_id, visited)),
            Some(TypeKey::Union(list_id)) | Some(TypeKey::Intersection(list_id)) => self
                .ctx
                .types
                .type_list(list_id)
                .iter()
                .any(|&member| self.type_contains_error_inner(member, visited)),
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.ctx.types.object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_error_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(ref index) = shape.string_index {
                    if self.type_contains_error_inner(index.value_type, visited) {
                        return true;
                    }
                }
                if let Some(ref index) = shape.number_index {
                    if self.type_contains_error_inner(index.value_type, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::Function(shape_id)) => {
                let shape = self.ctx.types.function_shape(shape_id);
                self.type_contains_error_inner(shape.return_type, visited)
            }
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.type_contains_error_inner(sig.return_type, visited))
                {
                    return true;
                }
                if shape
                    .construct_signatures
                    .iter()
                    .any(|sig| self.type_contains_error_inner(sig.return_type, visited))
                {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_error_inner(prop.type_id, visited))
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.ctx.types.type_application(app_id);
                if self.type_contains_error_inner(app.base, visited) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|&arg| self.type_contains_error_inner(arg, visited))
            }
            Some(TypeKey::Conditional(cond_id)) => {
                let cond = self.ctx.types.conditional_type(cond_id);
                self.type_contains_error_inner(cond.check_type, visited)
                    || self.type_contains_error_inner(cond.extends_type, visited)
                    || self.type_contains_error_inner(cond.true_type, visited)
                    || self.type_contains_error_inner(cond.false_type, visited)
            }
            Some(TypeKey::Mapped(mapped_id)) => {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                if self.type_contains_error_inner(mapped.constraint, visited) {
                    return true;
                }
                if let Some(name_type) = mapped.name_type {
                    if self.type_contains_error_inner(name_type, visited) {
                        return true;
                    }
                }
                self.type_contains_error_inner(mapped.template, visited)
            }
            Some(TypeKey::IndexAccess(base, index)) => {
                self.type_contains_error_inner(base, visited)
                    || self.type_contains_error_inner(index, visited)
            }
            Some(TypeKey::TemplateLiteral(template_id)) => self
                .ctx
                .types
                .template_list(template_id)
                .iter()
                .any(|span| match span {
                    TemplateSpan::Type(span_type) => {
                        self.type_contains_error_inner(*span_type, visited)
                    }
                    _ => false,
                }),
            Some(TypeKey::KeyOf(inner)) | Some(TypeKey::ReadonlyType(inner)) => {
                self.type_contains_error_inner(inner, visited)
            }
            Some(TypeKey::TypeParameter(info)) => {
                if let Some(constraint) = info.constraint {
                    if self.type_contains_error_inner(constraint, visited) {
                        return true;
                    }
                }
                if let Some(default) = info.default {
                    if self.type_contains_error_inner(default, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::Infer(info)) => {
                if let Some(constraint) = info.constraint {
                    if self.type_contains_error_inner(constraint, visited) {
                        return true;
                    }
                }
                if let Some(default) = info.default {
                    if self.type_contains_error_inner(default, visited) {
                        return true;
                    }
                }
                false
            }
            Some(TypeKey::Error) => true,
            Some(TypeKey::TypeQuery(_))
            | Some(TypeKey::UniqueSymbol(_))
            | Some(TypeKey::ThisType)
            | Some(TypeKey::Ref(_))
            | Some(TypeKey::Literal(_))
            | Some(TypeKey::Intrinsic(_))
            | None => false,
        }
    }

    fn implicit_any_return_display(&self, return_type: TypeId) -> String {
        if self.is_null_or_undefined_only(return_type) {
            return "any".to_string();
        }
        self.format_type(return_type)
    }

    fn should_report_implicit_any_return(&self, return_type: TypeId) -> bool {
        // Only report when return type is exactly 'any', not when it contains 'any' somewhere.
        // For example, Promise<void> should not trigger TS7010 even if Promise's definition
        // contains 'any' in its type structure.
        return_type == TypeId::ANY || self.is_null_or_undefined_only(return_type)
    }

    fn maybe_report_implicit_any_return(
        &mut self,
        name: Option<String>,
        name_node: Option<NodeIndex>,
        return_type: TypeId,
        has_type_annotation: bool,
        has_contextual_return: bool,
        fallback_node: NodeIndex,
    ) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if !self.ctx.no_implicit_any || has_type_annotation || has_contextual_return {
            return;
        }
        if !self.should_report_implicit_any_return(return_type) {
            return;
        }

        let return_text = self.implicit_any_return_display(return_type);
        if let Some(name) = name {
            let message = format_message(
                diagnostic_messages::IMPLICIT_ANY_RETURN,
                &[&name, &return_text],
            );
            self.error_at_node(
                name_node.unwrap_or(fallback_node),
                &message,
                diagnostic_codes::IMPLICIT_ANY_RETURN,
            );
        } else {
            let message = format_message(
                diagnostic_messages::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
                &[&return_text],
            );
            self.error_at_node(
                fallback_node,
                &message,
                diagnostic_codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
            );
        }
    }

    pub(crate) fn function_body_falls_through(&mut self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return true;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                return self.block_falls_through(&block.statements.nodes);
            }
        }
        false
    }

    fn block_falls_through(&mut self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if !self.statement_falls_through(stmt_idx) {
                return false;
            }
        }
        true
    }

    /// Check for unreachable code after return/throw statements in a block.
    /// Emits TS7027 for any statements that come after a return or throw.
    fn check_unreachable_code_in_block(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        let mut unreachable = false;
        for &stmt_idx in statements {
            if unreachable {
                // This statement is unreachable
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                    diagnostic_codes::UNREACHABLE_CODE_DETECTED,
                );
            } else {
                // Check if this statement makes subsequent statements unreachable
                let Some(node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                match node.kind {
                    syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                        unreachable = true;
                    }
                    syntax_kind_ext::EXPRESSION_STATEMENT => {
                        // Check if the expression is of type 'never' (e.g., throw(), assertNever())
                        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                            continue;
                        };
                        let expr_type = self.get_type_of_node(expr_stmt.expression);
                        if expr_type.is_never() {
                            unreachable = true;
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        // Check if any variable has a 'never' initializer
                        let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                            continue;
                        };
                        for &decl_idx in &var_stmt.declarations.nodes {
                            let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                                continue;
                            };
                            for &list_decl_idx in &var_list.declarations.nodes {
                                let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                                    continue;
                                };
                                let Some(decl) =
                                    self.ctx.arena.get_variable_declaration(list_decl_node)
                                else {
                                    continue;
                                };
                                if decl.initializer.is_none() {
                                    continue;
                                }
                                let init_type = self.get_type_of_node(decl.initializer);
                                if init_type.is_never() {
                                    unreachable = true;
                                    break;
                                }
                            }
                            if unreachable {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return true;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => false,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| self.block_falls_through(&block.statements.nodes))
                .unwrap_or(true),
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                    return true;
                };
                let expr_type = self.get_type_of_node(expr_stmt.expression);
                !expr_type.is_never()
            }
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                    return true;
                };
                for &decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                        continue;
                    };
                    for &list_decl_idx in &var_list.declarations.nodes {
                        let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                            continue;
                        };
                        let Some(decl) = self.ctx.arena.get_variable_declaration(list_decl_node)
                        else {
                            continue;
                        };
                        if decl.initializer.is_none() {
                            continue;
                        }
                        let init_type = self.get_type_of_node(decl.initializer);
                        if init_type.is_never() {
                            return false;
                        }
                    }
                }
                true
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return true;
                };
                let then_falls = self.statement_falls_through(if_data.then_statement);
                if if_data.else_statement.is_none() {
                    return true;
                }
                let else_falls = self.statement_falls_through(if_data.else_statement);
                then_falls || else_falls
            }
            syntax_kind_ext::SWITCH_STATEMENT => self.switch_falls_through(stmt_idx),
            syntax_kind_ext::TRY_STATEMENT => self.try_falls_through(stmt_idx),
            syntax_kind_ext::CATCH_CLAUSE => self
                .ctx
                .arena
                .get_catch_clause(node)
                .map(|catch_data| self.statement_falls_through(catch_data.block))
                .unwrap_or(true),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(node),
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => true,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.statement_falls_through(labeled.statement))
                .unwrap_or(true),
            _ => true,
        }
    }

    fn switch_falls_through(&mut self, switch_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(switch_idx) else {
            return true;
        };
        let Some(switch_data) = self.ctx.arena.get_switch(node) else {
            return true;
        };
        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return true;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return true;
        };

        let mut has_default = false;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                has_default = true;
            }
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };
            if self.block_falls_through(&clause.statements.nodes) {
                return true;
            }
        }

        !has_default
    }

    fn try_falls_through(&mut self, try_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(try_idx) else {
            return true;
        };
        let Some(try_data) = self.ctx.arena.get_try(node) else {
            return true;
        };

        let try_falls = self.statement_falls_through(try_data.try_block);
        let catch_falls = if !try_data.catch_clause.is_none() {
            self.statement_falls_through(try_data.catch_clause)
        } else {
            false
        };

        if !try_data.finally_block.is_none() {
            let finally_falls = self.statement_falls_through(try_data.finally_block);
            if !finally_falls {
                return false;
            }
        }

        try_falls || catch_falls
    }

    fn loop_falls_through(&mut self, node: &crate::parser::thin_node::ThinNode) -> bool {
        let Some(loop_data) = self.ctx.arena.get_loop(node) else {
            return true;
        };

        let condition_always_true = if loop_data.condition.is_none() {
            true
        } else {
            self.is_true_condition(loop_data.condition)
        };

        if condition_always_true && !self.contains_break_statement(loop_data.statement) {
            return false;
        }

        true
    }

    fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        node.kind == SyntaxKind::TrueKeyword as u16
    }

    fn contains_break_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::BREAK_STATEMENT => true,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .any(|&stmt| self.contains_break_statement(stmt))
                })
                .unwrap_or(false),
            syntax_kind_ext::IF_STATEMENT => self
                .ctx
                .arena
                .get_if_statement(node)
                .map(|if_data| {
                    self.contains_break_statement(if_data.then_statement)
                        || (!if_data.else_statement.is_none()
                            && self.contains_break_statement(if_data.else_statement))
                })
                .unwrap_or(false),
            // Don't recurse into switch statements - breaks inside target the switch, not outer loop
            syntax_kind_ext::SWITCH_STATEMENT => false,
            syntax_kind_ext::TRY_STATEMENT => self
                .ctx
                .arena
                .get_try(node)
                .map(|try_data| {
                    self.contains_break_statement(try_data.try_block)
                        || (!try_data.catch_clause.is_none()
                            && self.contains_break_statement(try_data.catch_clause))
                        || (!try_data.finally_block.is_none()
                            && self.contains_break_statement(try_data.finally_block))
                })
                .unwrap_or(false),
            // Don't recurse into nested loops - breaks inside target the nested loop, not outer loop
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => false,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.contains_break_statement(labeled.statement))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Infer the return type of a function body by collecting return expressions.
    fn infer_return_type_from_body(
        &mut self,
        body_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        if body_idx.is_none() {
            return TypeId::VOID; // No body - function returns void
        }

        let Some(node) = self.ctx.arena.get(body_idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind != syntax_kind_ext::BLOCK {
            return self.return_expression_type(body_idx, return_context);
        }

        let mut return_types = Vec::new();
        let mut saw_empty = false;

        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_return_types_in_statement(
                    stmt_idx,
                    &mut return_types,
                    &mut saw_empty,
                    return_context,
                );
            }
        }

        if return_types.is_empty() {
            return TypeId::VOID;
        }

        if saw_empty {
            return_types.push(TypeId::VOID);
        }

        self.ctx.types.union(return_types)
    }

    fn return_expression_type(
        &mut self,
        expr_idx: NodeIndex,
        return_context: Option<TypeId>,
    ) -> TypeId {
        let prev_context = self.ctx.contextual_type;
        if let Some(ctx_type) = return_context {
            self.ctx.contextual_type = Some(ctx_type);
        }
        let return_type = self.get_type_of_node(expr_idx);
        self.ctx.contextual_type = prev_context;
        return_type
    }

    fn collect_return_types_in_statement(
        &mut self,
        stmt_idx: NodeIndex,
        return_types: &mut Vec<TypeId>,
        saw_empty: &mut bool,
        return_context: Option<TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    if return_data.expression.is_none() {
                        *saw_empty = true;
                    } else {
                        let return_type =
                            self.return_expression_type(return_data.expression, return_context);
                        return_types.push(return_type);
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_return_types_in_statement(
                            stmt,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_return_types_in_statement(
                        if_data.then_statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if !if_data.else_statement.is_none() {
                        self.collect_return_types_in_statement(
                            if_data.else_statement,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node) {
                    if let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) {
                        if let Some(case_block) = self.ctx.arena.get_block(case_block_node) {
                            for &clause_idx in &case_block.statements.nodes {
                                if let Some(clause_node) = self.ctx.arena.get(clause_idx) {
                                    if let Some(clause) =
                                        self.ctx.arena.get_case_clause(clause_node)
                                    {
                                        for &stmt_idx in &clause.statements.nodes {
                                            self.collect_return_types_in_statement(
                                                stmt_idx,
                                                return_types,
                                                saw_empty,
                                                return_context,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_return_types_in_statement(
                        try_data.try_block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                    if !try_data.catch_clause.is_none() {
                        self.collect_return_types_in_statement(
                            try_data.catch_clause,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                    if !try_data.finally_block.is_none() {
                        self.collect_return_types_in_statement(
                            try_data.finally_block,
                            return_types,
                            saw_empty,
                            return_context,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_return_types_in_statement(
                        catch_data.block,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_return_types_in_statement(
                        loop_data.statement,
                        return_types,
                        saw_empty,
                        return_context,
                    );
                }
            }
            _ => {}
        }
    }

    /// Check if a function body has at least one return statement with a value.
    /// This is a simplified check - doesn't do full control flow analysis.
    fn body_has_return_with_value(&self, body_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        // For block bodies, check all statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(node) {
                return self.statements_have_return_with_value(&block.statements.nodes);
            }
        }

        false
    }

    /// Check if any statement in the list contains a return with a value.
    fn statements_have_return_with_value(&self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if self.statement_has_return_with_value(stmt_idx) {
                return true;
            }
        }
        false
    }

    /// Check if a statement contains a return with a value.
    fn statement_has_return_with_value(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    // Return with expression
                    return !return_data.expression.is_none();
                }
                false
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    return self.statements_have_return_with_value(&block.statements.nodes);
                }
                false
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    // Check both then and else branches
                    let then_has = self.statement_has_return_with_value(if_data.then_statement);
                    let else_has = if !if_data.else_statement.is_none() {
                        self.statement_has_return_with_value(if_data.else_statement)
                    } else {
                        false
                    };
                    return then_has || else_has;
                }
                false
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node) {
                    if let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) {
                        // Case block is stored as a Block containing case clauses
                        if let Some(case_block) = self.ctx.arena.get_block(case_block_node) {
                            for &clause_idx in &case_block.statements.nodes {
                                if let Some(clause_node) = self.ctx.arena.get(clause_idx) {
                                    if let Some(clause) =
                                        self.ctx.arena.get_case_clause(clause_node)
                                    {
                                        if self.statements_have_return_with_value(
                                            &clause.statements.nodes,
                                        ) {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                false
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    let try_has = self.statement_has_return_with_value(try_data.try_block);
                    let catch_has = if !try_data.catch_clause.is_none() {
                        self.statement_has_return_with_value(try_data.catch_clause)
                    } else {
                        false
                    };
                    let finally_has = if !try_data.finally_block.is_none() {
                        self.statement_has_return_with_value(try_data.finally_block)
                    } else {
                        false
                    };
                    return try_has || catch_has || finally_has;
                }
                false
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    return self.statement_has_return_with_value(catch_data.block);
                }
                false
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    return self.statement_has_return_with_value(loop_data.statement);
                }
                false
            }
            _ => false,
        }
    }
}
