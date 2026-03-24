//! # `CheckerState` - Type Checker Orchestration Layer
//!
//! This module serves as the orchestration layer for the TypeScript type checker.
//! It coordinates between various specialized checking modules while maintaining
//! shared state and caching for performance.
//!
//! ## Architecture - Modular Design
//!
//! The checker has been decomposed into focused modules, each responsible for
//! a specific aspect of type checking:
//!
//! ### Core Orchestration (This Module - state.rs)
//! - **Entry Points**: `check_source_file`, `check_statement`
//! - **Type Resolution**: `get_type_of_node`, `get_type_of_symbol`
//! - **Caching & Lifecycle**: `cache_symbol_type`, node type cache management
//! - **Delegation**: Coordinates calls to specialized modules
//!
//! ## Extracted Modules
//!
//! ### Type Computation (`type_computation.rs` - 3,189 lines)
//! - `get_type_of_binary_expression`
//! - `get_type_of_call_expression`
//! - `get_type_of_property_access`
//! - `get_type_of_element_access`
//! - `get_type_of_object_literal`
//! - `get_type_of_array_literal`
//! - And 30+ other type computation functions
//!
//! ### Type Checking (`type_checking.rs` - 9,556 lines)
//! - **Section 1-54**: Organized by functionality
//! - Declaration checking (classes, interfaces, enums)
//! - Statement checking (if, while, for, return)
//! - Property access validation
//! - Constructor checking
//! - Function signature validation
//!
//! ### Symbol Resolution (`symbol_resolver.rs` - 1,380 lines)
//! - `resolve_type_to_symbol`
//! - `resolve_value_symbol`
//! - `resolve_heritage_symbol`
//! - Private brand checking
//! - Import/Export resolution
//!
//! ### Flow Analysis (`flow_analysis.rs` - 1,511 lines)
//! - Definite assignment checking
//! - Type narrowing (typeof, discriminant)
//! - Control flow analysis
//! - TDZ (temporal dead zone) detection
//!
//! ### Error Reporting (`error_reporter.rs` - 1,923 lines)
//! - All `error_*` methods
//! - Diagnostic formatting
//! - Error reporting with detailed reasons
//!
//! ## Remaining in state.rs (~12,974 lines)
//!
//! The code remaining in this file is primarily:
//! 1. **Orchestration** (~4,000 lines): Entry points that coordinate between modules
//! 2. **Caching** (~2,000 lines): Node type cache, symbol type cache management
//! 3. **Dispatchers** (~3,000 lines): `compute_type_of_node` delegates to `type_computation` functions
//! 4. **Type Relations** (~2,000 lines): `is_assignable_to`, `is_subtype_of` (wrapper around solver)
//! 5. **Constructor/Class Helpers** (~2,000 lines): Complex type resolution for classes and inheritance
//!
//! ## Performance Optimizations
//!
//! - **Node Type Cache**: Avoids recomputing types for the same node
//! - **Symbol Type Cache**: Caches computed types for symbols
//! - **Fuel Management**: Prevents infinite loops and timeouts
//! - **Cycle Detection**: Detects circular type references
//!
//! ## Usage
//!
//! ```text
//! use crate::state::CheckerState;
//!
//! let mut checker = CheckerState::new(&arena, &binder, &types, file_name, options);
//! checker.check_source_file(root_idx);
//! ```
//!
//! # Step 12: Orchestration Layer Documentation ✅ COMPLETE
//!
//! **Date**: 2026-01-24
//! **Status**: Documentation complete
//! **Lines**: 12,974 (50.5% reduction from 26,217 original)
//! **Extracted**: 17,559 lines across 5 specialized modules
//!
//! The 2,000 line target was deemed unrealistic as the remaining code is
//! necessary orchestration that cannot be extracted without:
//! - Breaking the clean delegation pattern to specialized modules
//! - Creating circular dependencies between modules
//! - Duplicating shared state management code

use crate::CheckerContext;
use crate::context::{CheckerOptions, RequestCacheKey, TypingRequest};
use tsz_binder::BinderState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{QueryDatabase, TypeId, substitute_this_type};

thread_local! {
    /// Shared depth counter for all cross-arena delegation points.
    /// Prevents stack overflow from deeply nested CheckerState creation.
    static CROSS_ARENA_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// =============================================================================
// CheckerState
// =============================================================================

/// Type checker state using `NodeArena` and Solver type system.
///
/// This is a performance-optimized checker that works directly with the
/// cache-friendly Node architecture and uses the solver's `TypeInterner`
/// for structural type equality.
///
/// The state is stored in a `CheckerContext` which can be shared with
/// specialized checker modules (expressions, statements, declarations).
pub struct CheckerState<'a> {
    /// Shared checker context containing all state.
    pub ctx: CheckerContext<'a>,
}

// Re-export from centralized limits — do NOT redefine these here.
pub use tsz_common::limits::MAX_CALL_DEPTH;
pub use tsz_common::limits::MAX_INSTANTIATION_DEPTH;
pub use tsz_common::limits::MAX_TREE_WALK_ITERATIONS;
pub use tsz_common::limits::MAX_TYPE_RESOLUTION_OPS;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum EnumKind {
    Numeric,
    String,
    Mixed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberAccessLevel {
    Private,
    Protected,
}

#[derive(Clone, Debug)]
pub(crate) struct MemberAccessInfo {
    pub(crate) level: MemberAccessLevel,
    pub(crate) declaring_class_idx: NodeIndex,
    pub(crate) declaring_class_name: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberLookup {
    NotFound,
    Public,
    Restricted(MemberAccessLevel),
}

// Re-export flow analysis types for internal use
pub(crate) use crate::flow_analysis::{ComputedKey, PropertyKey};

/// Mode for resolving parameter types during extraction.
/// Used to consolidate duplicate parameter extraction functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParamTypeResolutionMode {
    /// Use `get_type_from_type_node_in_type_literal` - for type literal contexts
    InTypeLiteral,
    /// Use `get_type_from_type_node` - for declaration contexts
    FromTypeNode,
}

// =============================================================================
// AssignabilityOverrideProvider Implementation
// =============================================================================

/// Helper struct that implements `AssignabilityOverrideProvider` by delegating
/// to `CheckerState` methods. Captures the `TypeEnvironment` reference.
pub(crate) struct CheckerOverrideProvider<'a, 'b> {
    checker: &'a CheckerState<'b>,
    env: Option<&'a tsz_solver::TypeEnvironment>,
}

impl<'a, 'b> CheckerOverrideProvider<'a, 'b> {
    pub(crate) const fn new(
        checker: &'a CheckerState<'b>,
        env: Option<&'a tsz_solver::TypeEnvironment>,
    ) -> Self {
        Self { checker, env }
    }
}

impl<'a, 'b> tsz_solver::AssignabilityOverrideProvider for CheckerOverrideProvider<'a, 'b> {
    fn enum_assignability_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        self.checker.enum_assignability_override(source, target)
    }

    fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        self.checker
            .abstract_constructor_assignability_override(source, target, self.env)
    }

    fn constructor_accessibility_override(&self, source: TypeId, target: TypeId) -> Option<bool> {
        self.checker
            .constructor_accessibility_override(source, target, self.env)
    }
}

impl<'a> CheckerState<'a> {
    /// Create a new `CheckerState`.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `binder` - The binder state with symbols
    /// * `types` - The shared type interner (for thread-safe type deduplication)
    /// * `file_name` - The source file name
    /// * `compiler_options` - Compiler options for type checking
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::new(arena, binder, types, file_name, compiler_options),
        }
    }

    /// Create a new `CheckerState` with a shared `DefinitionStore`.
    ///
    /// This ensures that all type definitions (interfaces, type aliases, etc.) across
    /// different files and lib contexts share the same `DefId` namespace, preventing
    /// `DefId` collisions.
    ///
    /// # Arguments
    /// * `definition_store` - Shared `DefinitionStore` (wrapped in Arc for thread-safety)
    /// * Other args same as `new()`
    pub fn new_with_shared_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        definition_store: std::sync::Arc<tsz_solver::def::DefinitionStore>,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::new_with_shared_def_store(
                arena,
                binder,
                types,
                file_name,
                compiler_options,
                definition_store,
            ),
        }
    }

    /// Create a new `CheckerState` with a persistent cache.
    /// This allows reusing type checking results from previous queries.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `binder` - The binder state with symbols
    /// * `types` - The shared type interner
    /// * `file_name` - The source file name
    /// * `cache` - The persistent type cache from previous queries
    /// * `compiler_options` - Compiler options for type checking
    pub fn with_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: crate::TypeCache,
        compiler_options: CheckerOptions,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::with_cache(
                arena,
                binder,
                types,
                file_name,
                cache,
                compiler_options,
            ),
        }
    }

    /// Create a child `CheckerState` that shares the parent's caches.
    /// This is used for temporary checkers (e.g., cross-file symbol resolution)
    /// to ensure cache results are not lost (fixes Cache Isolation Bug).
    pub fn with_parent_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &Self,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::with_parent_cache(
                arena,
                binder,
                types,
                file_name,
                compiler_options,
                &parent.ctx,
            ),
        }
    }

    /// Thread-local guard for cross-arena delegation depth.
    /// All cross-arena delegation points (`delegate_cross_arena_symbol_resolution`,
    /// `get_type_params_for_symbol`, `type_of_value_declaration`) MUST call this
    /// before creating a child `CheckerState`. Returns true if delegation is allowed.
    pub(crate) fn enter_cross_arena_delegation() -> bool {
        let d = CROSS_ARENA_DEPTH.with(std::cell::Cell::get);
        if d >= 5 {
            return false;
        }
        CROSS_ARENA_DEPTH.with(|c| c.set(d + 1));
        true
    }

    /// Decrement the cross-arena delegation depth counter.
    pub(crate) fn leave_cross_arena_delegation() {
        CROSS_ARENA_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
    }

    fn should_apply_flow_narrowing_for_identifier(
        &self,
        idx: NodeIndex,
        skip_flow_narrowing: bool,
    ) -> bool {
        if skip_flow_narrowing {
            return false;
        }

        // When TS2454 was emitted for this node, check_flow_usage already returned
        // the declared type. Re-narrowing would override that with the narrowed type,
        // hiding assignment errors (TS2322) that tsc correctly emits.
        if self.ctx.daa_error_nodes.contains(&idx.0) {
            return false;
        }

        self.is_narrowable_identifier(idx)
    }

    /// Check if a node is a narrowable identifier (variable with flow analysis).
    /// This is pure — depends only on AST structure, not type-checking state.
    /// Results are cached per-NodeIndex to avoid 4-5 binder/arena lookups on
    /// repeated visits (e.g., 34 references to `options` in the same function).
    fn is_narrowable_identifier(&self, idx: NodeIndex) -> bool {
        if let Some(&cached) = self.ctx.narrowable_identifier_cache.borrow().get(&idx.0) {
            return cached;
        }
        let result = self.is_narrowable_identifier_uncached(idx);
        self.ctx
            .narrowable_identifier_cache
            .borrow_mut()
            .insert(idx.0, result);
        result
    }

    fn is_narrowable_identifier_uncached(&self, idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        let mut value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return true;
        }

        let Some(mut decl_node) = self.ctx.arena.get(value_decl) else {
            return true;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            value_decl = ext.parent;
            decl_node = parent_node;
        }

        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return true;
        }
        if !self.is_const_variable_declaration(value_decl) {
            return true;
        }

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return true;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return true;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return true;
        };
        !(init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
    }

    /// Check if we are currently inside a cross-arena delegation.
    /// Used to skip position-based checks (like TDZ) that compare node positions
    /// from different arenas.
    pub(crate) fn is_in_cross_arena_delegation() -> bool {
        CROSS_ARENA_DEPTH.with(|c| c.get() > 0)
    }

    /// Check if the source file has any parse errors.
    ///
    /// This flag is set by the driver before type checking based on parse diagnostics.
    /// It's used to suppress certain type-level diagnostics when the file
    /// has syntax errors (e.g., JSON files parsed as TypeScript).
    pub(crate) const fn has_parse_errors(&self) -> bool {
        self.ctx.has_parse_errors
    }

    /// Check if the source file has real syntax errors (not just conflict markers).
    /// Conflict markers (TS1185) are treated as trivia and don't affect AST structure,
    /// so they should not suppress TS2304 errors.
    pub(crate) const fn has_syntax_parse_errors(&self) -> bool {
        self.ctx.has_syntax_parse_errors
    }

    /// Check if a node's span overlaps with or is very close to a parse error position.
    /// Used to suppress cascading checker diagnostics (e.g. TS2391, TS2364) when the
    /// node is likely a parser-recovery artifact.
    /// Check if a parse error falls directly within the node's span (no margin).
    /// Used for tight suppression checks where the generous margin of
    /// `node_has_nearby_parse_error` would cause false positives.
    pub(crate) fn node_span_contains_parse_error(&self, idx: NodeIndex) -> bool {
        if !self.has_syntax_parse_errors() || self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            if err_pos >= node.pos && err_pos < node.end {
                return true;
            }
        }
        false
    }

    pub(crate) fn node_has_nearby_parse_error(&self, idx: NodeIndex) -> bool {
        if !self.has_syntax_parse_errors() || self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        // A generous window: if any parse error is within the node's span or up to
        // 8 bytes beyond it, consider the node tainted by parser recovery.
        const MARGIN: u32 = 8;
        let node_start = node.pos.saturating_sub(MARGIN);
        let node_end = node.end.saturating_add(MARGIN);
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            if err_pos >= node_start && err_pos <= node_end {
                return true;
            }
        }
        false
    }

    /// Check if ANY parse error (including non-suppressing ones like TS1359)
    /// falls within a node's span. Used for TS2456 suppression where reserved-
    /// word parse errors in type parameter lists should prevent false circularity.
    pub(crate) fn node_contains_any_parse_error(&self, idx: NodeIndex) -> bool {
        if !self.ctx.has_parse_errors || self.ctx.all_parse_error_positions.is_empty() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        for &err_pos in &self.ctx.all_parse_error_positions {
            if err_pos >= node.pos && err_pos < node.end {
                return true;
            }
        }
        false
    }

    /// Apply `this` type substitution to a method call's return type.
    ///
    /// When a method returns `this`, the return type should be the type of the receiver.
    /// For `obj.method()` where method returns `this`, we substitute `ThisType` with typeof obj.
    pub(crate) fn apply_this_substitution_to_call_return(
        &mut self,
        return_type: tsz_solver::TypeId,
        call_expression: tsz_parser::parser::NodeIndex,
    ) -> tsz_solver::TypeId {
        use tsz_solver::TypeId;

        // Fast path: intrinsic types can't contain ThisType
        if return_type.is_intrinsic() {
            return return_type;
        }

        // Try to extract the receiver from the call expression.
        // The call_expression parameter is actually the callee expression (call.expression),
        // which for method calls is a PropertyAccessExpression.
        // For `obj.method()`, this is `obj.method`, whose `.expression` is `obj`.
        let node = match self.ctx.arena.get(call_expression) {
            Some(n) => n,
            None => return return_type,
        };

        if let Some(access) = self.ctx.arena.get_access_expr(node) {
            let receiver_type = self.get_type_of_node(access.expression);
            if receiver_type != TypeId::ERROR && receiver_type != TypeId::ANY {
                return substitute_this_type(self.ctx.types, return_type, receiver_type);
            }
        }

        return_type
    }

    /// Create a new `CheckerState` with explicit compiler options.
    ///
    /// # Arguments
    /// * `arena` - The AST node arena
    /// * `binder` - The binder state with symbols
    /// * `types` - The shared type interner
    /// * `file_name` - The source file name
    /// * `compiler_options` - Compiler options for type checking
    pub fn with_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: &CheckerOptions,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::with_options(arena, binder, types, file_name, compiler_options),
        }
    }

    /// Create a new `CheckerState` with explicit compiler options and a shared `DefinitionStore`.
    ///
    /// This is used in parallel checking to ensure all files share the same `DefId` namespace.
    pub fn with_options_and_shared_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: &CheckerOptions,
        definition_store: std::sync::Arc<tsz_solver::def::DefinitionStore>,
    ) -> Self {
        let compiler_options = compiler_options.clone().apply_strict_defaults();
        CheckerState {
            ctx: CheckerContext::new_with_shared_def_store(
                arena,
                binder,
                types,
                file_name,
                compiler_options,
                definition_store,
            ),
        }
    }

    /// Create a new `CheckerState` with explicit compiler options and a persistent cache.
    pub fn with_cache_and_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: crate::TypeCache,
        compiler_options: &CheckerOptions,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::with_cache_and_options(
                arena,
                binder,
                types,
                file_name,
                cache,
                compiler_options,
            ),
        }
    }

    /// Create a new `CheckerState` with a persistent cache and a shared `DefinitionStore`.
    ///
    /// This combines cache restoration (for reusing type checking results across edits)
    /// with a shared definition store (for cross-file `DefId` consistency). This is the
    /// constructor the LSP uses for incremental re-checking with project-wide definitions.
    pub fn with_cache_and_shared_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: crate::TypeCache,
        compiler_options: CheckerOptions,
        definition_store: std::sync::Arc<tsz_solver::def::DefinitionStore>,
    ) -> Self {
        CheckerState {
            ctx: CheckerContext::with_cache_and_shared_def_store(
                arena,
                binder,
                types,
                file_name,
                cache,
                compiler_options,
                definition_store,
            ),
        }
    }

    /// Extract the persistent cache from this checker.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> crate::TypeCache {
        self.ctx.extract_cache()
    }

    // =========================================================================
    // Symbol Type Caching
    // =========================================================================

    /// Cache a computed symbol type for fast lookup and incremental type checking.
    ///
    /// This function stores the computed type of a symbol in the `symbol_types` cache,
    /// allowing subsequent lookups to avoid recomputing the type.
    ///
    /// ## Caching Strategy:
    /// - Types are cached after first computation
    /// - Cache key is the `SymbolId`
    /// - Cache persists for the lifetime of the type check
    ///
    /// ## Incremental Type Checking:
    /// - When a symbol changes, its cache entry is invalidated
    /// - Dependent symbols are re-computed on next access
    /// - Enables efficient re-typechecking of modified files
    ///
    /// ## Cache Invalidation:
    /// - Symbol modifications trigger dependency tracking
    /// - Dependent symbols are tracked via `record_symbol_dependency`
    /// - Cache is cleared for invalidated symbols
    ///
    /// ## Performance:
    /// - Avoids expensive type recomputation
    /// - Critical for performance in large codebases
    /// - Most symbol types are looked up multiple times
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// interface User {
    ///   name: string;
    ///   age: number;
    /// }
    /// let user: User;
    /// // First lookup: computes User type, caches it
    /// // Second lookup: returns cached User type (fast)
    ///
    /// function process(u: User) {
    ///   // User type parameter is cached
    ///   // Multiple uses of u resolve to the same cached type
    /// }
    /// ```
    pub(crate) fn cache_symbol_type(&mut self, sym_id: SymbolId, type_id: TypeId) {
        self.ctx.symbol_types.insert(sym_id, type_id);
    }

    pub(crate) fn record_symbol_dependency(&mut self, dependency: SymbolId) {
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

    pub(crate) fn push_symbol_dependency(&mut self, sym_id: SymbolId, clear_deps: bool) {
        if clear_deps {
            self.ctx.symbol_dependencies.remove(&sym_id);
        }
        self.ctx.symbol_dependency_stack.push(sym_id);
    }

    pub(crate) fn pop_symbol_dependency(&mut self) {
        self.ctx.symbol_dependency_stack.pop();
    }

    /// Infer and cache parameter types using contextual typing.
    ///
    /// This is needed for cases like:
    /// `export function filter<T>(arr: T[], predicate: (item: T) => boolean) { for (const item of arr) { ... } }`
    /// where `item`'s type comes from the contextual type of `arr`.
    pub(crate) fn infer_parameter_types_from_context(&mut self, params: &[NodeIndex]) {
        for &param_idx in params {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Only infer when there's no annotation and no default value.
            if param.type_annotation.is_some() || param.initializer.is_some() {
                continue;
            }

            let symbol_ids = self.parameter_symbol_ids(param_idx, param.name);
            let Some(sym_id) = symbol_ids.into_iter().flatten().next() else {
                continue;
            };

            // Skip destructuring parameters here (they are handled separately by binding pattern inference).
            if let Some(name_node) = self.ctx.arena.get(param.name)
                && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                continue;
            }

            // If we already have a concrete cached type, keep it.
            if let Some(&cached) = self.ctx.symbol_types.get(&sym_id)
                && cached != TypeId::UNKNOWN
                && cached != TypeId::ANY
                && cached != TypeId::ERROR
            {
                continue;
            }

            // Use contextual typing by resolving the parameter's identifier in its function scope.
            let inferred = self.get_type_of_identifier(param.name);
            if inferred != TypeId::UNKNOWN && inferred != TypeId::ERROR {
                for sym_id in self
                    .parameter_symbol_ids(param_idx, param.name)
                    .into_iter()
                    .flatten()
                {
                    self.cache_symbol_type(sym_id, inferred);
                }
            }
        }
    }

    /// Push an expected return type onto the stack when entering a function.
    ///
    /// This function is called when entering a function to track the expected
    /// return type. The stack is used to validate that all return statements
    /// are compatible with the function's declared return type.
    ///
    /// **Return Type Stack:**
    /// - Functions can be nested (inner functions, closures)
    /// - Stack tracks return type for each nesting level
    /// - Pushed when entering function, popped when exiting
    ///
    /// **Use Cases:**
    /// - Function declarations: `function foo(): string {}`
    /// - Function expressions: `const f = function(): number {}`
    /// - Arrow functions: `const f = (): boolean => {}`
    /// - Method declarations
    ///
    /// **Validation:**
    /// - Return statements are checked against the top of stack
    /// - Enables early error detection for mismatched return types
    ///
    pub fn push_return_type(&mut self, return_type: TypeId) {
        self.ctx.push_return_type(return_type);
    }

    /// Pop an expected return type from the stack when exiting a function.
    ///
    /// This function is called when exiting a function to remove the expected
    /// return type from the stack. This restores the previous return type for
    /// nested functions.
    ///
    /// **Stack Management:**
    /// - Pops the most recently pushed return type
    /// - Restores previous return type (for nested functions)
    /// - Must be called once per push (balanced push/pop)
    ///
    pub fn pop_return_type(&mut self) {
        self.ctx.pop_return_type();
    }

    /// Get the current expected return type if in a function.
    ///
    /// Returns the return type at the top of the return type stack.
    /// Returns None if not inside a function (stack is empty).
    ///
    /// **Use Cases:**
    /// - Validating return statements: `return value;`
    /// - Checking function body completeness
    /// - Contextual typing for return expressions
    ///
    /// **Nesting:**
    /// - Returns the innermost function's return type
    /// - Handles nested functions and closures correctly
    ///
    pub fn current_return_type(&self) -> Option<TypeId> {
        self.ctx.current_return_type()
    }

    // =========================================================================
    // Diagnostics (delegated to CheckerContext)
    // =========================================================================

    /// Add an error diagnostic to the diagnostics collection.
    ///
    /// This is the main entry point for reporting type errors. All error reporting
    /// flows through this function (directly or through helper functions).
    ///
    /// **Diagnostic Components:**
    /// - **start**: Byte offset of error start in file
    /// - **length**: Length of the error span in bytes
    /// - **message**: Human-readable error message
    /// - **code**: TypeScript error code (`TSxxxx`)
    ///
    /// **Error Categories:**
    /// - **Error**: Type errors that prevent compilation
    /// - **Warning**: Potential issues that don't prevent compilation
    /// - **Suggestion**: Code quality suggestions
    ///
    /// **Error Codes:**
    /// - TS2304: Cannot find name
    /// - TS2322: Type is not assignable
    /// - TS2339: Property does not exist
    /// - And many more...
    ///
    /// **Use Cases:**
    /// - Direct error emission: `self.error(start, length, message, 2304)`
    /// - Through helper functions: `error_cannot_find_name_at`, `error_type_not_assignable_at`, etc.
    /// - Error messages are formatted with type information
    ///
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        self.ctx.error(start, length, message, code);
    }

    /// Get the (start, end) span of a node for error reporting.
    ///
    /// This function retrieves the position information of an AST node,
    /// which is used for error reporting and IDE features.
    ///
    /// **Span Information:**
    /// - Returns `(start, end)` tuple of byte offsets
    /// - Start is the byte offset of the node's first character
    /// - End is the byte offset of the node's last character
    /// - Returns None if node doesn't exist in arena
    ///
    /// **Use Cases:**
    /// - Error reporting: `self.error(start, end - start, message, code)`
    /// - Diagnostic spans: Point to the problematic code
    /// - Quick info: Hover information for IDE
    /// - Code navigation: Jump to definition references
    ///
    pub fn get_node_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        self.ctx.get_node_span(idx)
    }

    /// Emit an error diagnostic at a specific source position.
    pub fn emit_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx
            .diagnostics
            .push(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                message.to_string(),
                code,
            ));
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

    // =========================================================================
    // Type Resolution - Core Methods
    // =========================================================================

    /// Get the type of a node.
    /// Get the type of an AST node with caching and circular reference detection.
    ///
    /// This is the main entry point for type computation. All type checking ultimately
    /// flows through this function to get the type of AST nodes.
    ///
    /// ## Caching:
    /// - Types are cached in `ctx.node_types` by node index
    /// - Subsequent calls for the same node return the cached type
    /// - Cache is checked first before computation
    ///
    /// ## Fuel Management:
    /// - Consumes fuel on each call to prevent infinite loops
    /// - Returns ERROR if fuel is exhausted (prevents type checker timeout)
    /// - Fuel is reset between file check operations
    ///
    /// ## Circular Reference Detection:
    /// - Tracks currently resolving nodes in `ctx.node_resolution_set`
    /// - Returns ERROR if a circular reference is detected
    /// - Helps expose type resolution bugs early
    ///
    /// ## Examples:
    /// ```typescript
    /// let x = 42;           // Type: number
    /// let y = x;            // Type: number (from cache)
    /// let z = x + y;        // Types: x=number, y=number, result=number
    /// ```
    ///
    /// ## Performance:
    /// - Caching prevents redundant type computation
    /// - Circular reference detection prevents infinite recursion
    /// - Fuel management ensures termination even for malformed code
    const fn request_cache_is_audited_access_kind(kind: u16) -> bool {
        kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    fn request_cache_key_for_node(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> Option<RequestCacheKey> {
        let key = RequestCacheKey::from_request(request)?;
        let Some(node) = self.ctx.arena.get(idx) else {
            self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
            return None;
        };

        let audited = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_element_access(idx)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_object_literal(idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_array_literal(idx)
            }
            _ => false,
        };

        if !audited {
            self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
            return None;
        }

        Some(key)
    }

    fn request_cache_lookup(
        &mut self,
        idx: NodeIndex,
        kind: u16,
        key: RequestCacheKey,
    ) -> Option<TypeId> {
        if Self::request_cache_is_audited_access_kind(kind) {
            self.ctx
                .request_cache_counters
                .property_access_request_cache_lookups += 1;
        }
        if let Some(&cached) = self.ctx.request_node_types.get(&(idx.0, key)) {
            self.ctx.request_cache_counters.request_cache_hits += 1;
            if Self::request_cache_is_audited_access_kind(kind) {
                self.ctx
                    .request_cache_counters
                    .property_access_request_cache_hits += 1;
            }
            return Some(cached);
        }
        self.ctx.request_cache_counters.request_cache_misses += 1;
        None
    }

    fn cache_request_type(&mut self, idx: NodeIndex, key: RequestCacheKey, ty: TypeId) {
        self.ctx.request_node_types.insert((idx.0, key), ty);
    }

    fn is_request_cache_safe_property_access(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if self.ctx.enclosing_class.is_some() || self.is_this_expression(access.expression) {
            return false;
        }
        if self.is_super_expression(access.expression) {
            return false;
        }
        if self
            .ctx
            .arena
            .get(access.name_or_argument)
            .is_some_and(|name| name.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16)
        {
            return false;
        }
        self.is_request_cache_safe_expression_tree(access.expression)
    }

    fn is_request_cache_safe_element_access(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if self.ctx.enclosing_class.is_some() || self.is_this_expression(access.expression) {
            return false;
        }
        if self.is_super_expression(access.expression) {
            return false;
        }
        self.is_request_cache_safe_expression_tree(access.expression)
            && self.is_request_cache_safe_expression_tree(access.name_or_argument)
    }

    fn is_request_cache_safe_object_literal(&self, idx: NodeIndex) -> bool {
        if self.ctx.in_destructuring_target
            || self.ctx.preserve_literal_types
            || self.current_this_type().is_some()
        {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        for &prop_idx in &obj.elements.nodes {
            let Some(prop_node) = self.ctx.arena.get(prop_idx) else {
                return false;
            };
            match prop_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(prop_node) else {
                        return false;
                    };
                    if self
                        .ctx
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                    {
                        return false;
                    }
                    if !self.is_request_cache_safe_expression_tree(prop.initializer) {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {}
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    let Some(spread) = self.ctx.arena.get_spread(prop_node) else {
                        return false;
                    };
                    if !self.is_request_cache_safe_expression_tree(spread.expression) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    fn is_request_cache_safe_array_literal(&self, idx: NodeIndex) -> bool {
        if self.ctx.in_destructuring_target || self.ctx.preserve_literal_types {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(array) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        for &elem_idx in &array.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let Some(spread) = self.ctx.arena.get_spread(elem_node) else {
                    return false;
                };
                if !self.is_request_cache_safe_expression_tree(spread.expression) {
                    return false;
                }
                continue;
            }
            if !self.is_request_cache_safe_expression_tree(elem_idx) {
                return false;
            }
        }
        true
    }

    fn is_request_cache_safe_expression_tree(&self, idx: NodeIndex) -> bool {
        if idx.is_none() {
            return true;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::TrueKeyword as u16
                || k == tsz_scanner::SyntaxKind::FalseKeyword as u16
                || k == tsz_scanner::SyntaxKind::NullKeyword as u16 =>
            {
                true
            }
            k if k == tsz_scanner::SyntaxKind::ThisKeyword as u16 => false,
            k if k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.ctx.arena.get_parenthesized(node).is_some_and(|paren| {
                    self.is_request_cache_safe_expression_tree(paren.expression)
                })
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(node)
                .is_some_and(|expr| self.is_request_cache_safe_expression_tree(expr.operand)),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_type_assertion(node)
                    .is_some_and(|expr| self.is_request_cache_safe_expression_tree(expr.expression))
                    || self.ctx.arena.get_unary_expr_ex(node).is_some_and(|expr| {
                        self.is_request_cache_safe_expression_tree(expr.expression)
                    })
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_property_access(idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.is_request_cache_safe_element_access(idx)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_object_literal(idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.is_request_cache_safe_array_literal(idx)
            }
            _ => false,
        }
    }

    pub fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_node_with_request(idx, &TypingRequest::NONE)
    }

    /// Compute the type of a node using an explicit [`TypingRequest`] instead of
    /// mutating ambient context fields.
    pub fn get_type_of_node_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let use_node_cache = request.is_empty();
        let request_cache_key = if use_node_cache {
            None
        } else {
            self.request_cache_key_for_node(idx, request)
        };
        let skip_flow_narrowing = request.flow.skip_flow_narrowing();

        if let Some(key) = request_cache_key
            && let Some(node) = self.ctx.arena.get(idx)
            && let Some(cached) = self.request_cache_lookup(idx, node.kind, key)
        {
            tracing::trace!(
                idx = idx.0,
                type_id = cached.0,
                "(request-cached) get_type_of_node"
            );
            return cached;
        }

        if use_node_cache && let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            let is_super_sensitive_access = self.ctx.arena.get(idx).is_some_and(|node| {
                use tsz_parser::parser::syntax_kind_ext;
                (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                    && self
                        .ctx
                        .arena
                        .get_access_expr(node)
                        .is_some_and(|access| self.is_super_expression(access.expression))
            });
            let is_super_keyword = self
                .ctx
                .arena
                .get(idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16);

            if is_super_sensitive_access || is_super_keyword {
                // `super` diagnostics depend on the current class-member context.
                // Reusing a silent cache entry from type-environment building can
                // suppress TS17011/TS2336/TS2855 on the checked path.
            }
            // PERF FAST PATH: Check the flow_analysis_cache directly with a cheap key
            // before doing the expensive should_apply_flow_narrowing_for_identifier check.
            // If the flow cache already has a result for this (flow_node, symbol, type),
            // we can return it immediately — skipping FlowAnalyzer creation, is_narrowable_identifier
            // checks, parameter default checks, and all other setup (~300ns savings per call).
            else if !skip_flow_narrowing
                && !self.ctx.daa_error_nodes.contains(&idx.0)
                && let Some(flow_node) = self.ctx.binder.get_node_flow(idx)
                && let Some(sym_id) = self
                    .ctx
                    .binder
                    .get_node_symbol(idx)
                    .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
            {
                let key = (flow_node, sym_id, cached);
                let flow_cached = self.ctx.flow_analysis_cache.borrow().get(&key).copied();
                if let Some(flow_cached) = flow_cached {
                    // Apply the same widening check as the full path
                    if flow_cached != cached && flow_cached != TypeId::ERROR {
                        let evaluated_cached = self.evaluate_type_for_assignability(cached);
                        let widened_cached =
                            tsz_solver::widening::widen_type(self.ctx.types, evaluated_cached);
                        if widened_cached == flow_cached {
                            return cached;
                        }
                    }
                    return flow_cached;
                }

                // PERF: Stable flow cache — skip flow analysis for repeated identifier
                // accesses in straight-line code where no narrowing occurs.
                // If a prior flow analysis for this symbol returned the declared type
                // unchanged, and the current flow node can reach that confirmed node
                // via a straight-line chain (no CONDITION/ASSIGNMENT/BRANCH_LABEL nodes),
                // we know flow analysis will return the declared type again.
                let stable_key = (sym_id, cached);
                let confirmed_flow = self
                    .ctx
                    .symbol_flow_confirmed
                    .borrow()
                    .get(&stable_key)
                    .copied();
                if let Some(confirmed_flow) = confirmed_flow
                    && self.is_straight_line_flow_to(flow_node, confirmed_flow, sym_id)
                {
                    // Update the confirmed flow node to the current one so
                    // the next access only needs to walk back a few steps.
                    self.ctx
                        .symbol_flow_confirmed
                        .borrow_mut()
                        .insert(stable_key, flow_node);
                    return cached;
                }
            }

            // CRITICAL FIX: For identifiers, apply flow narrowing to the cached type
            // Identifiers can have different types in different control flow branches.
            // Example: if (typeof x === "string") { x.toUpperCase(); }
            // The cache stores the declared type "string | number", but inside the if block,
            // x should have the narrowed type "string".
            //
            // Only apply narrowing if skip_flow_narrowing is false (respects testing/special contexts)
            let should_narrow =
                self.should_apply_flow_narrowing_for_identifier(idx, skip_flow_narrowing);

            if should_narrow {
                // Skip second flow narrowing if check_flow_usage already narrowed
                // this node.  Double-narrowing corrupts `any` types: e.g.
                // `any` → `string` (typeof), then re-narrowing `string` through
                // an instanceof guard produces `string & Object`.
                if self.ctx.flow_narrowed_nodes.contains(&idx.0) {
                    return cached;
                }
                let narrowed = self.apply_flow_narrowing(idx, cached);
                // FIX: If flow analysis returns a widened version of a literal cached type
                // (e.g., cached="foo" but flow returns string), use the cached type.
                // This prevents zombie freshness where flow analysis undoes literal preservation.
                // IMPORTANT: Evaluate the cached type first to expand type aliases
                // and lazy references, so widen_type can see the actual union members.
                if narrowed != cached && narrowed != TypeId::ERROR {
                    let evaluated_cached = self.evaluate_type_for_assignability(cached);
                    let widened_cached =
                        tsz_solver::widening::widen_type(self.ctx.types, evaluated_cached);
                    if widened_cached == narrowed {
                        // Update stable flow cache: flow returned declared type
                        self.update_symbol_flow_confirmed(idx, cached, true);
                        return cached;
                    }
                    // Flow returned a narrowed type — invalidate stable cache
                    self.update_symbol_flow_confirmed(idx, cached, false);
                } else {
                    // Flow returned declared type unchanged — update stable cache
                    self.update_symbol_flow_confirmed(idx, cached, true);
                }
                return narrowed;
            }

            // TS 5.1+ divergent accessor types: when in a write context
            // (skip_flow_narrowing is true, used by get_type_of_assignment_target),
            // property/element access nodes may have a different write type
            // than the cached read type. Bypass the cache so
            // get_type_of_property_access can return the write_type.
            if skip_flow_narrowing
                && self.ctx.arena.get(idx).is_some_and(|node| {
                    use tsz_parser::parser::syntax_kind_ext;
                    node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
                || is_super_sensitive_access
                || is_super_keyword
            {
                // Fall through to recompute with write-type awareness
            } else {
                tracing::trace!(idx = idx.0, type_id = cached.0, "(cached) get_type_of_node");
                return cached;
            }
        }

        // Check fuel - return ERROR if exhausted to prevent timeout
        if !self.ctx.consume_fuel() {
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
            if use_node_cache {
                self.ctx.node_types.insert(idx.0, TypeId::ERROR);
            } else if let Some(key) = request_cache_key {
                self.cache_request_type(idx, key, TypeId::ERROR);
            }
            return TypeId::ERROR;
        }

        // Check for circular reference - return ERROR to expose resolution bugs
        if self.ctx.node_resolution_set.contains(&idx) {
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
            if use_node_cache {
                self.ctx.node_types.insert(idx.0, TypeId::ERROR);
            } else if let Some(key) = request_cache_key {
                self.cache_request_type(idx, key, TypeId::ERROR);
            }
            return TypeId::ERROR;
        }

        // Push onto resolution stack
        self.ctx.node_resolution_stack.push(idx);
        self.ctx.node_resolution_set.insert(idx);

        // CRITICAL: Pre-cache ERROR placeholder to break deep recursion chains
        // This ensures that mid-resolution lookups get cached ERROR immediately
        // We'll overwrite this with the real result later (line 650)
        if use_node_cache {
            self.ctx.node_types.insert(idx.0, TypeId::ERROR);
        } else if let Some(key) = request_cache_key {
            self.cache_request_type(idx, key, TypeId::ERROR);
        }

        let result = self.compute_type_of_node_with_request(idx, request);

        // Pop from resolution stack
        self.ctx.node_resolution_stack.pop();
        self.ctx.node_resolution_set.remove(&idx);

        // Cache result - identifiers cache their DECLARED type,
        // but get_type_of_node applies flow narrowing when returning cached identifier types
        if use_node_cache {
            self.ctx.node_types.insert(idx.0, result);
        } else if let Some(key) = request_cache_key {
            self.cache_request_type(idx, key, result);
        } else if !request.is_empty() {
            // Contextual type was provided but no request cache key was generated
            // (e.g., call expressions are not request-cache-audited). Populate
            // node_types so a subsequent context-free lookup reuses the
            // contextually-inferred result instead of recomputing without context.
            // This prevents generic return type inference from being lost — e.g.,
            // querySelector<E>() returning E=Element instead of E=HTMLElement.
            if let Some(node) = self.ctx.arena.get(idx) {
                use tsz_parser::parser::syntax_kind_ext;
                if matches!(
                    node.kind,
                    syntax_kind_ext::CALL_EXPRESSION
                        | syntax_kind_ext::NON_NULL_EXPRESSION
                        | syntax_kind_ext::NEW_EXPRESSION
                ) {
                    self.ctx.node_types.insert(idx.0, result);
                }
            }
        }

        let should_narrow_computed =
            self.should_apply_flow_narrowing_for_identifier(idx, skip_flow_narrowing);

        if should_narrow_computed {
            // Skip second flow narrowing if check_flow_usage already narrowed
            // this node.  The compute result already has the correct narrowed type.
            if self.ctx.flow_narrowed_nodes.contains(&idx.0) {
                tracing::trace!(
                    idx = idx.0,
                    type_id = result.0,
                    "get_type_of_node (already flow-narrowed)"
                );
                return result;
            }

            // PERF: Stable flow cache — check if a prior flow analysis for this symbol
            // confirmed no narrowing (returned the declared type). If so, skip the
            // expensive FlowAnalyzer creation and flow graph walk.
            if let Some(flow_node) = self.ctx.binder.get_node_flow(idx)
                && let Some(sym_id) = self
                    .ctx
                    .binder
                    .get_node_symbol(idx)
                    .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
            {
                let stable_key = (sym_id, result);
                let confirmed_flow = self
                    .ctx
                    .symbol_flow_confirmed
                    .borrow()
                    .get(&stable_key)
                    .copied();
                if let Some(confirmed_flow) = confirmed_flow
                    && self.is_straight_line_flow_to(flow_node, confirmed_flow, sym_id)
                {
                    self.ctx
                        .symbol_flow_confirmed
                        .borrow_mut()
                        .insert(stable_key, flow_node);
                    // Also populate the flow_analysis_cache for this exact key
                    // so subsequent cached-path lookups are instant.
                    self.ctx
                        .flow_analysis_cache
                        .borrow_mut()
                        .insert((flow_node, sym_id, result), result);
                    return result;
                }
            }

            let mut narrowed = self.apply_flow_narrowing(idx, result);
            // FIX: Flow narrowing may return the original fresh type from the initializer
            // expression, undoing the freshness stripping that get_type_of_identifier
            // already performed. Re-apply freshness stripping to prevent "Zombie Freshness"
            // where excess property checks fire on non-literal variable references.
            if !self.ctx.compiler_options.sound_mode {
                use crate::query_boundaries::common::{is_fresh_object_type, widen_freshness};
                if is_fresh_object_type(self.ctx.types, narrowed) {
                    narrowed = widen_freshness(self.ctx.types, narrowed);
                }
            }
            // FIX: For mutable variables with non-widened literal declared types
            // (e.g., `declare var a: "foo"; let b = a` → b has declared type "foo"),
            // flow analysis may return the widened primitive (string) even though
            // there's no actual narrowing. Detect this case: if widen(result) == narrowed,
            // the flow is just widening our literal, not genuinely narrowing.
            // IMPORTANT: Evaluate the result type first to expand type aliases
            // and lazy references, so widen_type can see the actual union members.
            if narrowed != result && narrowed != TypeId::ERROR {
                let evaluated_result = self.evaluate_type_for_assignability(result);
                let widened_result =
                    tsz_solver::widening::widen_type(self.ctx.types, evaluated_result);
                if widened_result == narrowed {
                    // Flow just widened our literal type - use the original result
                    narrowed = result;
                }
            }
            // Update stable flow cache based on whether narrowing occurred
            if narrowed == result {
                self.update_symbol_flow_confirmed(idx, result, true);
            } else {
                self.update_symbol_flow_confirmed(idx, result, false);
            }
            tracing::trace!(
                idx = idx.0,
                type_id = result.0,
                narrowed_type_id = narrowed.0,
                "get_type_of_node (computed+narrowed)"
            );
            return narrowed;
        }

        tracing::trace!(idx = idx.0, type_id = result.0, "get_type_of_node");
        result
    }

    pub fn compute_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        self.compute_type_of_node_with_request(idx, &crate::context::TypingRequest::NONE)
    }

    /// Compute the type of a node under an explicit [`TypingRequest`].
    pub fn compute_type_of_node_with_request(
        &mut self,
        idx: NodeIndex,
        request: &crate::context::TypingRequest,
    ) -> TypeId {
        use crate::ExpressionChecker;

        let expr_result = {
            let mut expr_checker = ExpressionChecker::new(&mut self.ctx);
            expr_checker.compute_type_uncached_with_context(idx, request.contextual_type)
        };

        if expr_result != TypeId::DELEGATE {
            expr_result
        } else {
            self.compute_type_of_node_complex_with_request(idx, request)
        }
    }

    /// Like `get_type_of_function` but under an explicit [`TypingRequest`].
    pub fn get_type_of_function_with_request(
        &mut self,
        idx: NodeIndex,
        request: &crate::context::TypingRequest,
    ) -> TypeId {
        self.get_type_of_function_impl(idx, request)
    }

    /// Check if `from` can reach `to` via a flow chain that doesn't narrow `sym_id`.
    /// Returns true if the backward walk from `from` encounters no flow nodes that
    /// could change the type of `sym_id` (assignments to the symbol, loops, or calls).
    /// Walks at most 64 steps.
    ///
    /// Key insight: ASSIGNMENT nodes for OTHER symbols (e.g., `score += ...` for `score`
    /// while we track `options`) are safe to walk through. `BRANCH_LABEL` merge points
    /// from `??`/`?:` can be traversed by following their CONDITION antecedents.
    ///
    /// CONDITION nodes are only safe to traverse when reached FROM a `BRANCH_LABEL` (merge
    /// point), indicating they're part of a reconvergence pattern (like `??`). Direct
    /// CONDITION nodes (not from a merge) indicate entering a narrowing branch (like
    /// `if (typeof x === "string")`) and must block the walk.
    fn is_straight_line_flow_to(
        &self,
        from: tsz_binder::FlowNodeId,
        to: tsz_binder::FlowNodeId,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_binder::flow_flags;

        // Hard-stop flags: these always block because they introduce control flow
        // structures that could change any symbol's type.
        const HARD_STOP_FLAGS: u32 =
            flow_flags::LOOP_LABEL | flow_flags::SWITCH_CLAUSE | flow_flags::CALL;

        let mut current = from;
        // Track whether we reached the current node from a BRANCH_LABEL.
        // CONDITION nodes are only safe to traverse in this case.
        let mut from_branch_label = false;
        for _ in 0..64 {
            if current == to {
                return true;
            }
            let Some(flow) = self.ctx.binder.flow_nodes.get(current) else {
                return false;
            };

            // Loop labels, switch clauses, and calls always block
            if (flow.flags & HARD_STOP_FLAGS) != 0 {
                return false;
            }

            // ASSIGNMENT nodes: only block if they target our symbol
            if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                if self.stable_flow_assignment_targets_symbol(flow, sym_id) {
                    return false;
                }
                // Assignment to a different symbol — safe to pass through
                if flow.antecedent.len() == 1 {
                    from_branch_label = false;
                    current = flow.antecedent[0];
                    continue;
                }
                return false;
            }

            // CONDITION nodes: only safe when reached from a BRANCH_LABEL merge point.
            // This distinguishes ??/?:  reconvergence (BRANCH_LABEL -> CONDITION -> pre)
            // from entering narrowing branches (code -> CONDITION -> pre-if).
            if flow.has_any_flags(flow_flags::CONDITION) {
                if from_branch_label && flow.antecedent.len() == 1 {
                    from_branch_label = false;
                    current = flow.antecedent[0];
                    continue;
                }
                return false;
            }

            // BRANCH_LABEL merge points: walk through by following first antecedent.
            // For ??/?: reconvergence, all branches produce the same result for our
            // non-narrowed symbol, so following any single path is safe.
            if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                if !flow.antecedent.is_empty() {
                    from_branch_label = true;
                    current = flow.antecedent[0];
                    continue;
                }
                return false;
            }

            // ARRAY_MUTATION: pass through (doesn't affect identifier types)
            if flow.has_any_flags(flow_flags::ARRAY_MUTATION) {
                if flow.antecedent.len() == 1 {
                    from_branch_label = false;
                    current = flow.antecedent[0];
                    continue;
                }
                return false;
            }

            // Regular flow node — must have exactly one antecedent
            from_branch_label = false;
            if flow.antecedent.len() != 1 {
                return false;
            }
            current = flow.antecedent[0];
        }
        // Exceeded walk limit — conservatively return false
        false
    }

    /// Check if a flow ASSIGNMENT node targets the given symbol.
    fn stable_flow_assignment_targets_symbol(
        &self,
        flow: &tsz_binder::FlowNode,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        if flow.node.is_none() {
            return false;
        }
        // For assignments (x = ..., x += ...), the flow node references the LHS.
        // Check if that identifier's symbol matches our target symbol.
        if let Some(target_sym) = self.ctx.binder.get_node_symbol(flow.node).or_else(|| {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, flow.node)
        }) {
            return target_sym == sym_id;
        }
        // Can't determine the target — conservatively assume it targets our symbol
        true
    }

    /// Update the stable flow cache for a symbol after flow analysis.
    /// If `is_stable` is true (flow returned the declared type), record the current
    /// flow node. If false (narrowing occurred), remove the entry.
    fn update_symbol_flow_confirmed(&self, idx: NodeIndex, declared_type: TypeId, is_stable: bool) {
        if let Some(flow_node) = self.ctx.binder.get_node_flow(idx)
            && let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        {
            let key = (sym_id, declared_type);
            if is_stable {
                self.ctx
                    .symbol_flow_confirmed
                    .borrow_mut()
                    .insert(key, flow_node);
            } else {
                self.ctx.symbol_flow_confirmed.borrow_mut().remove(&key);
            }
        }
    }

    // Cache invalidation methods are in cache_invalidation.rs

    pub(crate) fn is_keyword_type_used_as_value_position(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        if matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION
                || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
        ) {
            return true;
        }

        // Recovery path: malformed value expressions like `number[]` are parsed
        // through ARRAY_TYPE wrappers, but still need TS2693 at the keyword.
        if parent_node.kind == syntax_kind_ext::ARRAY_TYPE {
            let Some(parent_ext) = self.ctx.arena.get_extended(parent) else {
                return false;
            };
            let grandparent = parent_ext.parent;
            if grandparent.is_none() {
                return false;
            }
            let Some(grandparent_node) = self.ctx.arena.get(grandparent) else {
                return false;
            };
            return matches!(
                grandparent_node.kind,
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::LABELED_STATEMENT
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION
                    || k == syntax_kind_ext::BINARY_EXPRESSION
                    || k == syntax_kind_ext::RETURN_STATEMENT
                    || k == syntax_kind_ext::VARIABLE_DECLARATION
                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
            );
        }

        false
    }

    /// Compute the type of a node (internal, not cached).
    ///
    /// This method first delegates to `ExpressionChecker` for expression type checking.
    /// If `ExpressionChecker` returns `TypeId::DELEGATE`, we fall back to the full
    /// `CheckerState` implementation that has access to symbol resolution, contextual
    /// typing, and other complex type checking features.
    /// Complex type computation that needs full `CheckerState` context.
    ///
    /// This is called when `ExpressionChecker` returns `TypeId::DELEGATE`,
    /// indicating the expression needs symbol resolution, contextual typing,
    /// or other features only available in `CheckerState`.
    #[allow(dead_code)]
    fn compute_type_of_node_complex(&mut self, idx: NodeIndex) -> TypeId {
        self.compute_type_of_node_complex_with_request(idx, &crate::context::TypingRequest::NONE)
    }

    fn compute_type_of_node_complex_with_request(
        &mut self,
        idx: NodeIndex,
        request: &crate::context::TypingRequest,
    ) -> TypeId {
        use crate::dispatch::ExpressionDispatcher;

        let mut dispatcher = ExpressionDispatcher::new(self);
        dispatcher.dispatch_type_computation_with_request(idx, request)
    }

    // Type resolution, type analysis, type environment, and checking methods
    // are in type_resolution/, type_analysis/, type_environment/,
    // state_checking.rs, and state_checking_members/
}
