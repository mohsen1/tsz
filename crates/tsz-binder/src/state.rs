//! Binder - Binder implementation using `NodeArena`.
//!
//! This is a clean implementation of the binder that works directly with
//! Node and `NodeArena`, avoiding the old Node enum pattern matching.

use crate::lib_loader;
use crate::module_resolution_debug::ModuleResolutionDebugger;
use crate::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeContext, ScopeId, Symbol, SymbolArena,
    SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{Level, debug, span};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Map from `(SymbolId, NodeIndex)` to the arena(s) containing that declaration.
/// Uses `SmallVec` to handle cross-arena `NodeIndex` collisions with zero overhead
/// for the common single-arena case.
pub type DeclarationArenaMap = FxHashMap<(SymbolId, NodeIndex), SmallVec<[Arc<NodeArena>; 1]>>;

const MAX_SCOPE_WALK_ITERATIONS: usize = 10_000;

type ReexportTarget = (String, Option<String>);
type FileReexports = FxHashMap<String, ReexportTarget>;
type FileReexportsMap = FxHashMap<String, FileReexports>;
type ExportCache = FxHashMap<(String, String), Option<SymbolId>>;
type IdentifierCache = FxHashMap<(usize, u32), Option<SymbolId>>;
type ExportCacheStorage = RwLock<ExportCache>;
type IdentifierCacheStorage = RwLock<IdentifierCache>;

/// Bitflags tracking which language features are used in a source file.
///
/// Populated by the binder during its AST walk (zero-cost at check time).
/// The checker queries these to decide whether to emit TS2318 diagnostics
/// for missing global types like `IterableIterator`, `TypedPropertyDescriptor`, etc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileFeatures(u8);

impl FileFeatures {
    pub const NONE: Self = Self(0);
    /// Source file contains generator functions (`function*`)
    pub const GENERATORS: Self = Self(1 << 0);
    /// Source file contains async generator functions (`async function*`)
    pub const ASYNC_GENERATORS: Self = Self(1 << 1);
    /// Source file contains decorator syntax (`@decorator`)
    pub const DECORATORS: Self = Self(1 << 2);
    /// Source file contains `using` declarations
    pub const USING: Self = Self(1 << 3);
    /// Source file contains `await using` declarations
    pub const AWAIT_USING: Self = Self(1 << 4);

    #[inline]
    #[must_use]
    pub const fn has(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    #[inline]
    pub const fn set(&mut self, flag: Self) {
        self.0 |= flag.0;
    }
}

/// Configuration options for the binder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinderOptions {
    /// ECMAScript target version.
    /// This affects language-specific behaviors like block-scoped function hoisting.
    pub target: ScriptTarget,
    /// When true, parse in strict mode and emit "use strict" for each source file.
    /// This mirrors the `--alwaysStrict` compiler option.
    pub always_strict: bool,
}

/// Lib file context for global type resolution.
/// This mirrors the definition in `checker::context` to avoid circular dependencies.
#[derive(Clone)]
pub struct LibContext {
    /// The AST arena for this lib file.
    pub arena: Arc<NodeArena>,
    /// The binder state with symbols from this lib file.
    pub binder: Arc<BinderState>,
}

/// Represents a module augmentation with arena context.
///
/// This structure ensures that `NodeIndex` values remain valid across files by
/// storing the source arena along with the augmentation declaration.
///
/// # Arena Context
///
/// `NodeIndex` is only valid within its specific `NodeArena`. When augmentations from
/// multiple files are merged, we need to preserve which arena each `NodeIndex` belongs to.
///
/// # Example
///
/// ```ignore
/// // File A: observable.d.ts
/// declare module "observable" {
///     interface Observable<T> {
///         filter(pred: (e:T) => boolean): Observable<T>;
///     }
/// }
///
/// // File B: map.ts
/// declare module "observable" {
///     interface Observable<T> {
///         map<U>(proj: (e:T) => U): Observable<U>;
///     }
/// }
/// ```
///
/// The augmentation for "Observable" should include both `filter` from File A's arena
/// and `map` from File B's arena.
/// Represents a global augmentation declaration from a `declare global {}` block.
/// For cross-file merging, the arena tracks which file's AST contains the declaration.
#[derive(Debug, Clone)]
pub struct GlobalAugmentation {
    /// Declaration node for this augmentation (interface/type alias inside `declare global {}`)
    pub node: NodeIndex,
    /// The arena containing this declaration (None = current file's arena, Some = cross-file)
    pub arena: Option<Arc<NodeArena>>,
}

impl GlobalAugmentation {
    /// Create a new global augmentation without arena context (during binding).
    #[must_use]
    pub const fn new(node: NodeIndex) -> Self {
        Self { node, arena: None }
    }

    /// Create a new global augmentation with arena context (during merge).
    #[must_use]
    pub const fn with_arena(node: NodeIndex, arena: Arc<NodeArena>) -> Self {
        Self {
            node,
            arena: Some(arena),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModuleAugmentation {
    /// Name of the augmented interface/type member (e.g., "map", "filter")
    pub name: String,
    /// Declaration node for this augmentation
    pub node: NodeIndex,
    /// The arena containing this declaration (None during binding, populated during merge)
    pub arena: Option<Arc<NodeArena>>,
}

impl ModuleAugmentation {
    /// Create a new module augmentation without arena context (during binding).
    #[must_use]
    pub const fn new(name: String, node: NodeIndex) -> Self {
        Self {
            name,
            node,
            arena: None,
        }
    }

    /// Create a new module augmentation with arena context (during merge).
    #[must_use]
    pub const fn with_arena(name: String, node: NodeIndex, arena: Arc<NodeArena>) -> Self {
        Self {
            name,
            node,
            arena: Some(arena),
        }
    }
}

/// Binder state using `NodeArena`.
pub struct BinderState {
    /// Binder options (ES target, etc.)
    pub options: BinderOptions,
    /// Arena for symbol storage
    pub symbols: SymbolArena,
    /// Current symbol table (local scope)
    pub current_scope: SymbolTable,
    /// Stack of parent scopes
    pub(crate) scope_stack: Vec<SymbolTable>,
    /// File-level locals (for module resolution)
    pub file_locals: SymbolTable,
    /// Expando property assignments: maps identifier name → set of property names
    /// that were assigned via `X.prop = value` patterns (single-level property access).
    /// Used to suppress false TS2339 errors on read-side property accesses.
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
    /// Ambient module declarations by specifier (e.g. "pkg", "./types")
    pub declared_modules: FxHashSet<String>,
    /// Whether the current source file is an external module (has top-level import/export).
    pub is_external_module: bool,
    /// Whether the current scope is in strict mode (via "use strict" directive or --alwaysStrict).
    /// In strict mode, function declarations inside blocks are block-scoped, not hoisted.
    pub(crate) is_strict_scope: bool,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Current flow node
    pub(crate) current_flow: FlowNodeId,
    /// Unreachable flow node
    pub(crate) unreachable_flow: FlowNodeId,
    /// Scope chain - stack of scope contexts (legacy, for hoisting)
    pub(crate) scope_chain: Vec<ScopeContext>,
    /// Current scope index in `scope_chain`
    pub(crate) current_scope_idx: usize,
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Symbol-to-arena mapping for cross-file declaration lookup (legacy, stores last arena)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (`SymbolId`, `NodeIndex` of declaration) -> Arena(s) containing that declaration
    /// This is needed when a symbol (like Array) is declared across multiple lib files.
    /// Uses `SmallVec` to handle cross-arena `NodeIndex` collisions: when two lib files have
    /// their interface declaration at the same `NodeIndex`, both arenas are stored.
    pub declaration_arenas: DeclarationArenaMap,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    /// Used by the checker for control flow analysis (type narrowing)
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Flow node after each top-level statement (for incremental binding).
    pub(crate) top_level_flow: FxHashMap<u32, FlowNodeId>,
    /// Map case/default clause nodes to their containing switch statement.
    pub switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    /// Hoisted var declarations
    pub(crate) hoisted_vars: Vec<(String, NodeIndex)>,
    /// Hoisted function declarations
    pub(crate) hoisted_functions: Vec<NodeIndex>,

    // ===== Persistent Scope System (for stateless checking) =====
    /// Persistent scopes - enables querying scope information without traversal order
    pub scopes: Vec<Scope>,
    /// Map from AST node (that creates a scope) to its `ScopeId`
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Current active `ScopeId` during binding
    pub current_scope_id: ScopeId,

    // ===== Module Resolution Debugging =====
    /// Debugger for tracking symbol table operations and scope lookups
    pub debugger: ModuleResolutionDebugger,

    // ===== Global Augmentations =====
    /// Tracks interface/type declarations inside `declare global` blocks that should
    /// merge with lib.d.ts symbols. Maps interface name to augmentation declarations.
    pub global_augmentations: FxHashMap<String, Vec<GlobalAugmentation>>,

    /// Flag indicating we're currently binding inside a `declare global` block
    pub(crate) in_global_augmentation: bool,

    // ===== Module Augmentations (Rule #44) =====
    /// Tracks interface/type declarations inside `declare module 'x'` blocks that should
    /// merge with the target module's symbols. Maps module specifier to augmentations.
    pub module_augmentations: FxHashMap<String, Vec<ModuleAugmentation>>,

    /// Flag indicating we're currently binding inside a module augmentation block
    pub(crate) in_module_augmentation: bool,

    /// The module specifier being augmented (set when `in_module_augmentation` is true)
    pub(crate) current_augmented_module: Option<String>,

    /// Lib binders for automatic lib symbol resolution.
    /// When `get_symbol()` doesn't find a symbol locally, it checks these lib binders.
    pub lib_binders: Vec<Arc<Self>>,

    /// Symbol IDs that originated from lib files.
    /// Used by `get_symbol()` to check `lib_binders` first for these IDs,
    /// avoiding collision with local symbols at the same index.
    pub lib_symbol_ids: FxHashSet<SymbolId>,

    /// Reverse mapping from user-local lib symbol IDs to (`lib_binder_ptr`, `original_local_id`).
    /// This allows Phase 2 of `merge_bind_results` to find the Phase 1 global ID for each
    /// user-local lib symbol. Built during `merge_lib_contexts_into_binder`.
    pub lib_symbol_reverse_remap: FxHashMap<SymbolId, (usize, SymbolId)>,

    /// Module exports: maps file names to their exported symbols for cross-file module resolution
    /// This enables resolving imports like `import { X } from './file'` where './file' is another file
    pub module_exports: FxHashMap<String, SymbolTable>,

    /// Re-exports: tracks `export { x } from 'module'` declarations
    /// Maps (`current_file`, `exported_name`) -> (`source_module`, `original_name`)
    /// Example: ("./a.ts", "foo", "./b.ts") means a.ts re-exports "foo" from b.ts
    pub reexports: FileReexportsMap,

    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps `current_file` -> Vec of `source_modules`
    /// A file can have multiple wildcard re-exports (e.g., `export * from 'a'; export * from 'b'`)
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,

    /// Cache for resolved exports to avoid repeated lookups through re-export chains.
    /// Key: (`module_specifier`, `export_name`) -> resolved `SymbolId` (or None if not found)
    /// This cache dramatically speeds up barrel file imports where the same export
    /// is looked up multiple times across different files.
    /// Uses `RwLock` for thread-safety in parallel compilation.
    resolved_export_cache: ExportCacheStorage,
    /// Cache for identifier resolution by AST node.
    /// Key: (`arena_pointer`, `node_index`) -> resolved `SymbolId` (or None if not found).
    /// This avoids repeated scope walks for hot checker paths that ask for the same
    /// identifier symbol many times (e.g. large switch/flow analysis files).
    pub(crate) resolved_identifier_cache: IdentifierCacheStorage,

    /// Shorthand ambient modules: modules declared with just `declare module "xxx"` (no body)
    /// Imports from these modules should resolve to `any` type
    pub shorthand_ambient_modules: FxHashSet<String>,

    /// Modules that use `export =` syntax (CommonJS-style exports)
    /// Used by the import checker to validate require-style imports
    pub modules_with_export_equals: FxHashSet<String>,
    /// Classification for modules with `export =`:
    /// true when the module resolves to a non-module entity.
    pub module_export_equals_non_module: FxHashMap<String, bool>,

    /// Flag indicating lib symbols have been merged into this binder's symbol arena.
    /// When true, `get_symbol()` should prefer local symbols over `lib_binders` lookups,
    /// since all lib symbols now have unique IDs in the local arena.
    pub(crate) lib_symbols_merged: bool,

    /// Break targets for control flow analysis.
    /// When we enter a loop or switch, we push a merge label that break statements jump to.
    pub(crate) break_targets: Vec<FlowNodeId>,

    /// Language features detected during binding (generators, decorators, using, etc.).
    /// Populated during `bind_source_file` with zero overhead since the binder already walks every node.
    pub file_features: FileFeatures,
}

/// Validation result describing issues found in the symbol table
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// A node->symbol mapping points to a non-existent symbol
    BrokenSymbolLink { node_index: u32, symbol_id: u32 },
    /// A symbol exists but has no declarations (orphaned)
    OrphanedSymbol { symbol_id: u32, name: String },
    /// A symbol's `value_declaration` points to a non-existent node
    InvalidValueDeclaration { symbol_id: u32, name: String },
}

/// Statistics about symbol resolution attempts and successes.
#[derive(Debug, Clone, Default)]
pub struct ResolutionStats {
    /// Total number of resolution attempts
    pub attempts: u64,
    /// Number of successful resolutions in scopes
    pub scope_hits: u64,
    /// Number of successful resolutions in `file_locals`
    pub file_local_hits: u64,
    /// Number of successful resolutions in `lib_binders`
    pub lib_binder_hits: u64,
    /// Number of failed resolutions
    pub failures: u64,
}

#[derive(Debug, Default)]
pub struct BinderStateScopeInputs {
    pub scopes: Vec<Scope>,
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    pub global_augmentations: FxHashMap<String, Vec<GlobalAugmentation>>,
    pub module_augmentations: FxHashMap<String, Vec<ModuleAugmentation>>,
    pub module_exports: FxHashMap<String, SymbolTable>,
    pub reexports: FileReexportsMap,
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    pub declaration_arenas: DeclarationArenaMap,
    pub shorthand_ambient_modules: FxHashSet<String>,
    pub modules_with_export_equals: FxHashSet<String>,
    pub flow_nodes: FlowNodeArena,
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    pub switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
}

impl BinderStateScopeInputs {
    fn with_scopes(scopes: Vec<Scope>, node_scope_ids: FxHashMap<u32, ScopeId>) -> Self {
        Self {
            scopes,
            node_scope_ids,
            flow_nodes: FlowNodeArena::new(),
            ..Self::default()
        }
    }
}

impl BinderState {
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(BinderOptions::default())
    }

    #[must_use]
    pub fn with_options(options: BinderOptions) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        let mut binder = Self {
            options,
            symbols: SymbolArena::new(),
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals: SymbolTable::new(),
            expando_properties: FxHashMap::default(),
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols: FxHashMap::default(),
            symbol_arenas: FxHashMap::default(),
            declaration_arenas: FxHashMap::default(),
            node_flow: FxHashMap::default(),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Vec::new(),
            node_scope_ids: FxHashMap::default(),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: FxHashMap::default(),
            in_global_augmentation: false,
            module_augmentations: FxHashMap::default(),
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules: FxHashSet::default(),
            modules_with_export_equals: FxHashSet::default(),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            file_features: FileFeatures::NONE,
        };
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Reset binder state to its initial values.
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier/export caches are poisoned when clearing
    /// their locks.
    pub fn reset(&mut self) {
        self.symbols.clear();
        self.current_scope.clear();
        self.scope_stack.clear();
        self.file_locals.clear();
        self.expando_properties.clear();
        self.declared_modules.clear();
        self.is_external_module = false;
        self.is_strict_scope = false;
        self.flow_nodes.clear();
        self.unreachable_flow = self.flow_nodes.alloc(flow_flags::UNREACHABLE);
        self.current_flow = FlowNodeId::NONE;
        self.scope_chain.clear();
        self.current_scope_idx = 0;
        self.node_symbols.clear();
        self.symbol_arenas.clear();
        self.declaration_arenas.clear();
        self.node_flow.clear();
        self.top_level_flow.clear();
        self.switch_clause_to_switch.clear();
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
        self.scopes.clear();
        self.node_scope_ids.clear();
        self.current_scope_id = ScopeId::NONE;
        self.debugger.clear();
        self.global_augmentations.clear();
        self.in_global_augmentation = false;
        self.module_augmentations.clear();
        self.in_module_augmentation = false;
        self.current_augmented_module = None;
        self.lib_binders.clear();
        self.lib_symbol_ids.clear();
        self.module_exports.clear();
        self.reexports.clear();
        self.wildcard_reexports.clear();
        self.resolved_export_cache.write().unwrap().clear();
        self.resolved_identifier_cache.write().unwrap().clear();
        self.shorthand_ambient_modules.clear();
        self.modules_with_export_equals.clear();
        self.module_export_equals_non_module.clear();
        self.lib_symbols_merged = false;
        self.break_targets.clear();
    }

    /// Set the current file name for debugging purposes.
    /// This should be called before binding a source file.
    pub fn set_debug_file(&mut self, file_name: &str) {
        self.debugger.set_current_file(file_name);
    }

    /// Get the module resolution debug summary.
    /// Returns a human-readable summary of all recorded debug events.
    pub fn get_debug_summary(&self) -> String {
        self.debugger.get_summary()
    }

    /// Get the arena for a specific declaration of a symbol.
    ///
    /// For symbols that are declared across multiple lib files (e.g., `Array` which is
    /// declared in es5.d.ts, es2015.core.d.ts, etc.), each declaration may be in a
    /// different arena. This method returns the correct arena for a specific declaration.
    ///
    /// Falls back to `symbol_arenas` (which stores the last arena for the symbol) if
    /// no specific declaration arena is found.
    ///
    /// Returns `None` if no arena is found for this symbol/declaration.
    pub fn get_arena_for_declaration(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> Option<&Arc<NodeArena>> {
        // First try the precise declaration-to-arena mapping
        if let Some(arena) = self
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|v| v.first())
        {
            return Some(arena);
        }
        // Fall back to symbol-level arena (for backwards compatibility and non-merged symbols)
        self.symbol_arenas.get(&sym_id)
    }

    /// Create a `BinderState` from pre-parsed lib data.
    ///
    /// This is used for loading pre-parsed lib files where we only have
    /// symbols and `file_locals` (no `node_symbols` or other binding state).
    #[must_use]
    pub fn from_preparsed(symbols: SymbolArena, file_locals: SymbolTable) -> Self {
        Self::from_bound_state(symbols, file_locals, FxHashMap::default())
    }

    /// Create a `BinderState` from existing bound state.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// The symbols and `node_symbols` come from the merged program state.
    #[must_use]
    pub fn from_bound_state(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        Self::from_bound_state_with_options(
            BinderOptions::default(),
            symbols,
            file_locals,
            node_symbols,
        )
    }

    /// Create a `BinderState` from existing bound state with options.
    #[must_use]
    pub fn from_bound_state_with_options(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
    ) -> Self {
        let mut flow_nodes = FlowNodeArena::new();
        let unreachable_flow = flow_nodes.alloc(flow_flags::UNREACHABLE);

        let mut binder = Self {
            options,
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            expando_properties: FxHashMap::default(),
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            symbol_arenas: FxHashMap::default(),
            declaration_arenas: FxHashMap::default(),
            node_flow: FxHashMap::default(),
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch: FxHashMap::default(),
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes: Vec::new(),
            node_scope_ids: FxHashMap::default(),
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations: FxHashMap::default(),
            in_global_augmentation: false,
            module_augmentations: FxHashMap::default(),
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports: FxHashMap::default(),
            reexports: FxHashMap::default(),
            wildcard_reexports: FxHashMap::default(),
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules: FxHashSet::default(),
            modules_with_export_equals: FxHashSet::default(),
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            file_features: FileFeatures::NONE,
        };
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Create a `BinderState` from existing bound state, preserving scopes.
    #[must_use]
    pub fn from_bound_state_with_scopes(
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        scopes: Vec<Scope>,
        node_scope_ids: FxHashMap<u32, ScopeId>,
    ) -> Self {
        Self::from_bound_state_with_scopes_and_augmentations(
            BinderOptions::default(),
            symbols,
            file_locals,
            node_symbols,
            BinderStateScopeInputs::with_scopes(scopes, node_scope_ids),
        )
    }

    /// Create a `BinderState` from existing bound state, preserving scopes and global augmentations.
    ///
    /// This is used for type checking after parallel binding and symbol merging.
    /// Global augmentations are interface/type declarations inside `declare global` blocks
    /// that should merge with lib.d.ts symbols during type resolution.
    /// Module augmentations are interface/type declarations inside `declare module 'x'` blocks
    /// that should merge with the target module's symbols.
    #[must_use]
    pub fn from_bound_state_with_scopes_and_augmentations(
        options: BinderOptions,
        symbols: SymbolArena,
        file_locals: SymbolTable,
        node_symbols: FxHashMap<u32, SymbolId>,
        inputs: BinderStateScopeInputs,
    ) -> Self {
        let BinderStateScopeInputs {
            scopes,
            node_scope_ids,
            global_augmentations,
            module_augmentations,
            module_exports,
            reexports,
            wildcard_reexports,
            symbol_arenas,
            declaration_arenas,
            shorthand_ambient_modules,
            modules_with_export_equals,
            flow_nodes,
            node_flow,
            switch_clause_to_switch,
            expando_properties,
        } = inputs;

        // Find the unreachable flow node in the existing flow_nodes, or create a new one
        let unreachable_flow = flow_nodes.find_unreachable().unwrap_or(
            // This shouldn't happen in practice since the binder always creates an unreachable flow
            FlowNodeId::NONE,
        );

        let mut binder = Self {
            options,
            symbols,
            current_scope: SymbolTable::new(),
            scope_stack: Vec::new(),
            file_locals,
            expando_properties,
            declared_modules: FxHashSet::default(),
            is_external_module: false,
            is_strict_scope: false,
            flow_nodes,
            current_flow: FlowNodeId::NONE,
            unreachable_flow,
            scope_chain: Vec::new(),
            current_scope_idx: 0,
            node_symbols,
            symbol_arenas,
            declaration_arenas,
            node_flow,
            top_level_flow: FxHashMap::default(),
            switch_clause_to_switch,
            hoisted_vars: Vec::new(),
            hoisted_functions: Vec::new(),
            scopes,
            node_scope_ids,
            current_scope_id: ScopeId::NONE,
            debugger: ModuleResolutionDebugger::new(),
            global_augmentations,
            in_global_augmentation: false,
            module_augmentations,
            in_module_augmentation: false,
            current_augmented_module: None,
            lib_binders: Vec::new(),
            lib_symbol_ids: FxHashSet::default(),
            lib_symbol_reverse_remap: FxHashMap::default(),
            module_exports,
            reexports,
            wildcard_reexports,
            resolved_export_cache: std::sync::RwLock::new(FxHashMap::default()),
            resolved_identifier_cache: std::sync::RwLock::new(FxHashMap::default()),
            shorthand_ambient_modules,
            modules_with_export_equals,
            module_export_equals_non_module: FxHashMap::default(),
            lib_symbols_merged: false,
            break_targets: Vec::new(),
            file_features: FileFeatures::NONE,
        };
        binder.recompute_module_export_equals_non_module();
        binder
    }

    /// Resolve an identifier to a symbol by walking up the persistent scope tree.
    /// This method enables stateless checking - the checker can query scope information
    /// without maintaining a traversal-order-dependent stack.
    ///
    /// Returns the `SymbolId` for the identifier, or None if not found.
    ///
    /// Debug logging (P1 Task):
    /// When debug mode is enabled, logs:
    /// - Scope chain traversal
    /// - Falls through to `file_locals`
    /// - Falls through to `lib_binders`
    /// - Resolution failures
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn resolve_identifier(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<SymbolId> {
        // Fast path: identifier resolution is pure for a fixed binder + arena.
        // Cache both hits and misses to avoid repeated scope walks in checker hot paths.
        let cache_key = (std::ptr::from_ref::<NodeArena>(arena) as usize, node_idx.0);
        if let Some(&cached) = self
            .resolved_identifier_cache
            .read()
            .unwrap()
            .get(&cache_key)
        {
            return cached;
        }

        let _span = span!(Level::DEBUG, "resolve_identifier", node_idx = node_idx.0).entered();

        let result = 'resolve: {
            // Get the identifier text
            let name = if let Some(ident) = arena.get_identifier_at(node_idx) {
                &ident.escaped_text
            } else {
                break 'resolve None;
            };

            debug!("[RESOLVE] Looking up identifier '{}'", name);

            if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
                // Walk up the scope chain
                let mut scope_depth = 0;
                while scope_id.is_some() {
                    if let Some(scope) = self.scopes.get(scope_id.0 as usize) {
                        if let Some(sym_id) = scope.table.get(name) {
                            debug!(
                                "[RESOLVE] '{}' FOUND in scope at depth {} (id={})",
                                name, scope_depth, sym_id.0
                            );
                            // Resolve import if this symbol is imported from another module
                            if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                                break 'resolve Some(resolved);
                            }
                            break 'resolve Some(sym_id);
                        }
                        scope_id = scope.parent;
                        scope_depth += 1;
                    } else {
                        break;
                    }
                }
            }

            // Fallback for bound-state binders without persistent scopes.
            if let Some(sym_id) = self.resolve_parameter_fallback(arena, node_idx, name) {
                debug!(
                    "[RESOLVE] '{}' FOUND via parameter fallback (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Finally check file locals / globals
            if let Some(sym_id) = self.file_locals.get(name) {
                debug!(
                    "[RESOLVE] '{}' FOUND in file_locals (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Chained lookup: check lib binders for global symbols
            // This enables resolving console, Array, Object, etc. from lib.d.ts
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    debug!(
                        "[RESOLVE] '{}' FOUND in lib_binder[{}] (id={}) - LIB SYMBOL",
                        name, i, sym_id.0
                    );
                    // Note: lib symbols are not imports, so no need to resolve
                    break 'resolve Some(sym_id);
                }
            }

            // Symbol not found - log the failure
            debug!(
                "[RESOLVE] '{}' NOT FOUND - searched scopes, file_locals, and {} lib binders",
                name,
                self.lib_binders.len()
            );

            None
        };

        self.resolved_identifier_cache
            .write()
            .unwrap()
            .insert(cache_key, result);

        result
    }

    /// Resolve an identifier by walking scopes and invoking a filter callback on candidates.
    ///
    /// This keeps scope traversal in the binder while allowing callers (checker) to
    /// apply contextual filtering (e.g., value-only vs type-only, class member filtering).
    pub fn resolve_identifier_with_filter<F>(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        lib_binders: &[Arc<Self>],
        mut accept: F,
    ) -> Option<SymbolId>
    where
        F: FnMut(SymbolId) -> bool,
    {
        let node = arena.get(node_idx)?;
        let name = if let Some(ident) = arena.get_identifier(node) {
            ident.escaped_text.as_str()
        } else {
            return None;
        };

        let mut consider =
            |sym_id: SymbolId| -> Option<SymbolId> { accept(sym_id).then_some(sym_id) };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };

                if let Some(sym_id) = scope.table.get(name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }

                if scope.kind == ContainerKind::Module
                    && let Some(container_sym_id) = self.get_node_symbol(scope.container_node)
                    && let Some(container_symbol) =
                        self.get_symbol_with_libs(container_sym_id, lib_binders)
                    && let Some(exports) = container_symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(name)
                    && let Some(found) = consider(member_id)
                {
                    return Some(found);
                }

                scope_id = scope.parent;
            }
        }

        if let Some(sym_id) = self.file_locals.get(name)
            && let Some(found) = consider(sym_id)
        {
            return Some(found);
        }

        if !self.lib_symbols_merged {
            for lib_binder in lib_binders {
                if let Some(sym_id) = lib_binder.file_locals.get(name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }
            }
        }

        None
    }

    /// Collect visible symbol names for diagnostics and suggestions.
    /// If `meaning_flags` is non-zero, only include symbols whose flags overlap with `meaning_flags`.
    pub fn collect_visible_symbol_names(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Vec<String> {
        self.collect_visible_symbol_names_filtered(arena, node_idx, 0)
    }

    /// Collect visible symbol names filtered by meaning flags.
    /// If `meaning_flags` is non-zero, only include symbols whose flags overlap with `meaning_flags`.
    pub fn collect_visible_symbol_names_filtered(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        meaning_flags: u32,
    ) -> Vec<String> {
        let mut names = FxHashSet::default();

        let passes_filter = |sym_id: &SymbolId| -> bool {
            if meaning_flags == 0 {
                return true;
            }
            self.get_symbol(*sym_id)
                .is_none_or(|sym| sym.flags & meaning_flags != 0)
        };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };
                for (symbol_name, sym_id) in scope.table.iter() {
                    if passes_filter(sym_id) {
                        names.insert(symbol_name.clone());
                    }
                }
                scope_id = scope.parent;
            }
        }

        for (symbol_name, sym_id) in self.file_locals.iter() {
            if passes_filter(sym_id) {
                names.insert(symbol_name.clone());
            }
        }

        names.into_iter().collect()
    }

    /// Resolve private identifiers (#foo) across class scopes.
    ///
    /// Returns (`symbols_found`, `saw_class_scope`).
    pub fn resolve_private_identifier_symbols(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        let Some(node) = arena.get(node_idx) else {
            return (Vec::new(), false);
        };
        let name = match arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return (Vec::new(), false),
        };

        let mut symbols = Vec::new();
        let mut saw_class_scope = false;
        let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) else {
            return (symbols, saw_class_scope);
        };

        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > MAX_SCOPE_WALK_ITERATIONS {
                break;
            }
            let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
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

    pub(crate) fn resolve_parameter_fallback(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        if self.scopes.is_empty() {
            let mut current = node_idx;
            while current.is_some() {
                let node = arena.get(current)?;
                if let Some(func) = arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        let param = arena.get_parameter_at(param_idx)?;
                        let ident = arena.get_identifier_at(param.name)?;
                        if ident.escaped_text == name {
                            return self.node_symbols.get(&param.name.0).copied();
                        }
                    }
                }
                let ext = arena.get_extended(current)?;
                current = ext.parent;
            }
        }
        None
    }

    /// Resolve an imported symbol to its actual export from the source module.
    ///
    /// When a symbol is imported (e.g., `import { foo } from './file'`), the binder creates
    /// a local ALIAS symbol with `import_module` set to './file'. This method resolves that
    /// alias to the actual exported symbol from the source module by looking up `module_exports`
    /// and following re-export chains.
    ///
    /// Returns the resolved `SymbolId`, or the original `sym_id` if it's not an import or resolution fails.
    pub(crate) fn resolve_import_if_needed(&self, sym_id: SymbolId) -> Option<SymbolId> {
        // Get the symbol to check if it's an import
        let sym = self.symbols.get(sym_id)?;
        let module_specifier = sym.import_module.as_ref()?;

        // Determine the export name:
        // - If import_name is set, use it (for renamed imports like `import { foo as bar }`)
        // - Otherwise use the symbol's escaped_name
        let export_name = sym.import_name.as_ref().unwrap_or(&sym.escaped_name);

        // Try to resolve the import, following re-export chains
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import by name from a module, following re-export chains.
    ///
    /// This function handles:
    /// - Direct exports: `export { foo }` - looks up in `module_exports`
    /// - Named re-exports: `export { foo } from 'bar'` - follows the re-export mapping
    /// - Wildcard re-exports: `export * from 'bar'` - searches the re-exported module
    ///
    /// Results are cached to speed up repeated lookups (common with barrel files).
    pub(crate) fn resolve_import_with_reexports(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        // Check cache first for fast path
        let cache_key = (module_specifier.to_string(), export_name.to_string());
        if let Some(&cached) = self.resolved_export_cache.read().unwrap().get(&cache_key) {
            return cached;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let result =
            self.resolve_import_with_reexports_inner(module_specifier, export_name, &mut visited);

        // Cache the result (including None for not found)
        self.resolved_export_cache
            .write()
            .expect("resolved_export_cache RwLock poisoned")
            .insert(cache_key, result);
        result
    }

    /// Inner implementation with cycle detection for module re-exports.
    pub(crate) fn resolve_import_with_reexports_inner(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<SymbolId> {
        let _span =
            span!(Level::DEBUG, "resolve_import_with_reexports", %module_specifier, %export_name)
                .entered();

        // Cycle detection: check if we've already visited this (module, export) pair
        let key = (module_specifier.to_string(), export_name.to_string());
        if visited.contains(&key) {
            return None;
        }
        visited.insert(key);

        // First, check if it's a direct export from this module
        if let Some(module_table) = self.module_exports.get(module_specifier)
            && let Some(sym_id) = module_table.get(export_name)
        {
            debug!(
                "[RESOLVE_IMPORT] '{}' from module '{}' -> direct export symbol id={}",
                export_name, module_specifier, sym_id.0
            );
            return Some(sym_id);
        }

        // Not found in direct exports, check for named re-exports
        if let Some(file_reexports) = self.reexports.get(module_specifier) {
            // Check for named re-export: `export { foo } from 'bar'`
            if let Some((source_module, original_name)) = file_reexports.get(export_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> following named re-export from '{}', original name='{}'",
                    export_name, module_specifier, source_module, name_to_lookup
                );
                return self.resolve_import_with_reexports_inner(
                    source_module,
                    name_to_lookup,
                    visited,
                );
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // A module can have multiple wildcard re-exports, check all of them
        if let Some(source_modules) = self.wildcard_reexports.get(module_specifier) {
            for source_module in source_modules {
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> trying wildcard re-export from '{}'",
                    export_name, module_specifier, source_module
                );
                if let Some(result) =
                    self.resolve_import_with_reexports_inner(source_module, export_name, visited)
                {
                    return Some(result);
                }
            }
        }

        // Export not found
        debug!(
            "[RESOLVE_IMPORT] '{}' from module '{}' -> NOT FOUND",
            export_name, module_specifier
        );
        None
    }

    /// Public method for testing import resolution with reexports.
    /// This allows tests to verify that wildcard and named re-exports are properly resolved.
    pub fn resolve_import_if_needed_public(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import symbol to its target, following re-export chains.
    ///
    /// This is used by the checker to resolve imported symbols to their actual declarations,
    /// following both named re-exports (`export { foo } from 'bar'`) and wildcard re-exports
    /// (`export * from 'bar'`).
    ///
    /// Returns the resolved `SymbolId` if found, None otherwise.
    pub fn resolve_import_symbol(&self, sym_id: SymbolId) -> Option<SymbolId> {
        self.resolve_import_if_needed(sym_id)
    }

    /// Find the enclosing scope for a given node by walking up the AST.
    /// Returns the `ScopeId` of the nearest scope-creating ancestor node.
    pub fn find_enclosing_scope(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;

        // Walk up the AST using parent pointers to find the nearest scope
        while current.is_some() {
            // Check if this node creates a scope
            if let Some(&scope_id) = self.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }

            // Move to parent node
            if let Some(_node) = arena.get(current) {
                if let Some(ext) = arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // If no scope found, return the root scope (index 0) if it exists
        (!self.scopes.is_empty()).then_some(ScopeId(0))
    }

    /// Enter a new persistent scope (in addition to legacy scope chain).
    /// This method is called when binding begins for a scope-creating node.
    pub(crate) fn enter_persistent_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        // Create new scope linked to current
        let new_scope_id =
            ScopeId(u32::try_from(self.scopes.len()).expect("persistent scope count exceeds u32"));
        let new_scope = Scope::new(self.current_scope_id, kind, node);
        self.scopes.push(new_scope);

        // Map node to this scope
        if node.is_some() {
            self.node_scope_ids.insert(node.0, new_scope_id);
        }

        // Update current scope
        self.current_scope_id = new_scope_id;
    }

    /// Exit the current persistent scope.
    pub(crate) fn exit_persistent_scope(&mut self) {
        if self.current_scope_id.is_some()
            && let Some(scope) = self.scopes.get(self.current_scope_id.0 as usize)
        {
            self.current_scope_id = scope.parent;
        }
    }

    /// Declare a symbol in the current persistent scope.
    /// This adds the symbol to the persistent scope table for later querying.
    pub(crate) fn declare_in_persistent_scope(&mut self, name: String, sym_id: SymbolId) {
        if self.current_scope_id.is_some()
            && let Some(scope) = self.scopes.get_mut(self.current_scope_id.0 as usize)
        {
            scope.table.set(name, sym_id);
        }
    }

    pub(crate) fn sync_current_scope_to_persistent(&mut self) {
        if self.current_scope_id.is_none() {
            return;
        }
        if let Some(persistent_scope) = self.scopes.get_mut(self.current_scope_id.0 as usize) {
            for (name, &sym_id) in self.current_scope.iter() {
                persistent_scope.table.set(name.clone(), sym_id);
            }
        }
    }

    pub(crate) fn source_file_is_external_module(arena: &NodeArena, root: NodeIndex) -> bool {
        let Some(source) = arena.get_source_file_at(root) else {
            return false;
        };

        for &stmt_idx in &source.statements.nodes {
            if stmt_idx.is_none() {
                continue;
            }
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
            if Self::is_node_exported(arena, stmt_idx) {
                return true;
            }
        }

        false
    }

    /// Check if a list of statements starts with a "use strict" prologue directive.
    /// Prologue directives are string literal expression statements at the top of a scope.
    fn has_use_strict_prologue(arena: &NodeArena, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break; // Prologues must be at the top
            }
            let Some(expr_stmt) = arena.get_expression_statement(stmt) else {
                break;
            };
            let Some(expr) = arena.get(expr_stmt.expression) else {
                break;
            };
            if expr.kind == SyntaxKind::StringLiteral as u16 {
                if let Some(lit) = arena.get_literal(expr)
                    && lit.text == "use strict"
                {
                    return true;
                }
            } else {
                break; // Non-string expression, stop looking for prologues
            }
        }
        false
    }

    /// Bind a source file using `NodeArena`.
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
        // Binding mutates scope/symbol tables, so stale identifier resolution entries
        // from prior passes must be dropped.
        self.resolved_identifier_cache.write().unwrap().clear();

        // Preserve lib symbols that were merged before binding (e.g., in parallel.rs)
        // When merge_lib_symbols is called before bind_source_file, lib symbols are stored
        // in file_locals and need to be preserved across the binding process.
        let lib_symbols: FxHashMap<String, SymbolId> = self
            .file_locals
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let has_lib_symbols = !lib_symbols.is_empty();

        // Initialize scope chain with source file scope (legacy)
        self.scope_chain.clear();
        self.scope_chain
            .push(ScopeContext::new(ContainerKind::SourceFile, root, None));
        self.current_scope_idx = 0;
        self.current_scope = SymbolTable::new();

        // Initialize persistent scope system
        self.scopes.clear();
        self.node_scope_ids.clear();
        self.current_scope_id = ScopeId::NONE;
        self.top_level_flow.clear();

        // Create root persistent scope for the source file
        self.enter_persistent_scope(ContainerKind::SourceFile, root);

        // Pre-populate root persistent scope with lib symbols if they were merged before binding
        if has_lib_symbols {
            if let Some(root_scope) = self.scopes.first_mut() {
                for (name, sym_id) in &lib_symbols {
                    root_scope.table.set(name.clone(), *sym_id);
                }
            }

            // Also merge lib symbols into current_scope for immediate availability
            // This ensures symbols like console, Array, Promise are available during binding
            for (name, sym_id) in &lib_symbols {
                if !self.current_scope.has(name) {
                    self.current_scope.set(name.clone(), *sym_id);
                }
            }
        }

        // Create START flow node for the file
        let start_flow = self.flow_nodes.alloc(flow_flags::START);
        self.current_flow = start_flow;
        self.is_external_module = Self::source_file_is_external_module(arena, root);

        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            // Detect strict mode: "use strict" prologue or --alwaysStrict option
            self.is_strict_scope = self.options.always_strict
                || Self::has_use_strict_prologue(arena, &sf.statements.nodes);

            // First pass: collect hoisted declarations
            self.collect_hoisted_declarations(arena, &sf.statements);

            // Process hoisted function declarations first (for hoisting)
            self.process_hoisted_functions(arena);

            // Process hoisted var declarations (for hoisting)
            self.process_hoisted_vars(arena);

            // Second pass: bind each statement
            for &stmt_idx in &sf.statements.nodes {
                self.bind_node(arena, stmt_idx);
                self.top_level_flow.insert(stmt_idx.0, self.current_flow);
            }

            // Populate module_exports for cross-file import resolution
            // This enables type-only import elision and proper import validation
            let file_name = sf.file_name.clone();
            self.populate_module_exports_from_file_symbols(arena, &file_name);
            self.recompute_module_export_equals_non_module();
        }

        self.sync_current_scope_to_persistent();

        // Store file locals from the ROOT scope only, not nested namespaces/modules.
        // This prevents namespace-local symbols from being accessible globally.
        // User symbols take precedence - only add lib symbols if no user symbol exists.
        let existing_file_locals = std::mem::take(&mut self.file_locals);

        // Only collect symbols from the root SourceFile scope, not nested namespaces/modules
        let root_scope_symbols = if let Some(root_scope) = self.scopes.first() {
            // The first scope is always the SourceFile scope
            root_scope.table.clone()
        } else {
            // Fallback: empty scope if no scopes exist (shouldn't happen)
            SymbolTable::new()
        };

        // Debug: log what's going into file_locals
        if std::env::var("BIND_DEBUG").is_ok() {
            debug!(
                "[FILE_LOCALS] Root scope has {} symbols",
                root_scope_symbols.len()
            );
            for (name, _) in root_scope_symbols.iter() {
                debug!("[FILE_LOCALS]   - {}", name);
            }
        }

        self.file_locals = root_scope_symbols;

        // Merge back any existing file locals (e.g., lib symbols) that were pre-populated.
        for (name, sym_id) in existing_file_locals.iter() {
            if !self.file_locals.has(name) {
                self.file_locals.set(name.clone(), *sym_id);
            }
        }

        // Restore lib symbols from the saved lib_symbols map (if they were pre-merged).
        if has_lib_symbols {
            for (name, sym_id) in &lib_symbols {
                if !self.file_locals.has(name) {
                    self.file_locals.set(name.clone(), *sym_id);
                }
            }
        }
    }

    /// Populate `module_exports` from file-level module symbols.
    ///
    /// This enables cross-file import resolution and type-only import elision.
    /// After binding a source file, we collect all module-level exports and
    /// add them to the `module_exports` table keyed by the file name.
    ///
    /// # Arguments
    /// * `arena` - The `NodeArena` containing the AST
    /// * `file_name` - The name of the file being bound (used as the key in `module_exports`)
    fn populate_module_exports_from_file_symbols(&mut self, _arena: &NodeArena, file_name: &str) {
        use crate::symbol_flags;

        // Collect all exports from all module-level symbols in this file
        let mut file_exports = SymbolTable::new();
        let mut export_equals_target: Option<SymbolId> = None;

        // Iterate through file_locals to find modules and their exports
        for (name, &sym_id) in self.file_locals.iter() {
            if name == "export=" {
                export_equals_target = Some(sym_id);
            }
            if let Some(symbol) = self.symbols.get(sym_id) {
                // Check if this is a module/namespace symbol
                if (symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                    != 0
                {
                    // If the module has an exports table, merge it into file_exports
                    if let Some(module_exports) = symbol.exports.as_ref() {
                        for (export_name, &export_sym_id) in module_exports.iter() {
                            if !file_exports.has(export_name) {
                                file_exports.set(export_name.clone(), export_sym_id);
                            }
                        }
                    }
                }

                // Also collect symbols that are explicitly exported via `export { X }`
                // or `export` modifier. These may not be module/namespace symbols but
                // need to be in module_exports for cross-file import resolution.
                if (symbol.is_exported || name == "export=") && !file_exports.has(name) {
                    file_exports.set(name.clone(), sym_id);
                }
            }
        }

        // `export = target` should expose namespace members from `target`.
        if let Some(target_sym_id) = export_equals_target
            && let Some(target_symbol) = self.symbols.get(target_sym_id)
        {
            if let Some(target_exports) = target_symbol.exports.as_ref() {
                for (export_name, &export_sym_id) in target_exports.iter() {
                    if !file_exports.has(export_name) {
                        file_exports.set(export_name.clone(), export_sym_id);
                    }
                }
            }
            if let Some(target_members) = target_symbol.members.as_ref() {
                for (member_name, &member_sym_id) in target_members.iter() {
                    if !file_exports.has(member_name) {
                        file_exports.set(member_name.clone(), member_sym_id);
                    }
                }
            }
        }

        // Add to module_exports if we found any exports
        if !file_exports.is_empty() {
            self.module_exports
                .insert(file_name.to_string(), file_exports);
        }
    }

    fn symbol_has_namespace_shape(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.symbols.get(sym_id) else {
            return false;
        };

        if (symbol.flags
            & (symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
            != 0
        {
            return true;
        }

        if symbol.exports.as_ref().is_some_and(|tbl| !tbl.is_empty())
            || symbol.members.as_ref().is_some_and(|tbl| !tbl.is_empty())
        {
            return true;
        }

        let mut declarations = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
            declarations.push(symbol.value_declaration);
        }

        declarations.into_iter().any(|decl_idx| {
            if decl_idx.is_none() {
                return false;
            }
            let Some(arena) = self
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
            else {
                return false;
            };
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module_decl) = arena.get_module(node) else {
                return false;
            };
            if module_decl.body.is_none() {
                return false;
            }
            let Some(body_node) = arena.get(module_decl.body) else {
                return false;
            };
            if body_node.kind == syntax_kind_ext::MODULE_BLOCK
                && let Some(block) = arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                return !statements.nodes.is_empty();
            }
            true
        })
    }

    fn compute_module_export_equals_non_module(&self, exports: &SymbolTable) -> Option<bool> {
        let export_assignment_targets = |sym: &Symbol| -> Vec<String> {
            let mut targets = Vec::new();
            let mut declarations = sym.declarations.clone();
            if sym.value_declaration.is_some() && !declarations.contains(&sym.value_declaration) {
                declarations.push(sym.value_declaration);
            }

            for decl_idx in declarations {
                if decl_idx.is_none() {
                    continue;
                }
                let Some(arena) = self
                    .declaration_arenas
                    .get(&(sym.id, decl_idx))
                    .and_then(|v| v.first())
                else {
                    continue;
                };
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                    continue;
                }
                let Some(assign) = arena.get_export_assignment(node) else {
                    continue;
                };
                if !assign.is_export_equals {
                    continue;
                }
                let Some(expr_node) = arena.get(assign.expression) else {
                    continue;
                };
                let Some(id) = arena.get_identifier(expr_node) else {
                    continue;
                };
                if !targets.contains(&id.escaped_text) {
                    targets.push(id.escaped_text.clone());
                }
            }

            targets
        };

        let export_equals_sym_id = exports.get("export=")?;

        let export_equals_symbol = self.symbols.get(export_equals_sym_id)?;

        let mut target_names = Vec::new();
        if !export_equals_symbol.escaped_name.is_empty() {
            target_names.push(export_equals_symbol.escaped_name.clone());
        }
        for target_name in export_assignment_targets(export_equals_symbol) {
            if !target_names.contains(&target_name) {
                target_names.push(target_name);
            }
        }

        let has_distinct_named_exports = exports.iter().any(|(name, _)| {
            name != "export=" && !target_names.iter().any(|target| target == name)
        });

        let mut candidate_ids = Vec::new();
        let mut push_candidate = |candidate_id: SymbolId| {
            if !candidate_ids.contains(&candidate_id) {
                candidate_ids.push(candidate_id);
            }
        };

        push_candidate(export_equals_sym_id);
        for target_name in &target_names {
            for candidate_id in self.symbols.find_all_by_name(target_name) {
                push_candidate(candidate_id);
            }
        }

        let has_namespace_shape = candidate_ids
            .into_iter()
            .any(|candidate_id| self.symbol_has_namespace_shape(candidate_id));

        Some(!has_namespace_shape && !has_distinct_named_exports)
    }

    /// Recompute `export =` non-module classification for all known module exports.
    pub fn recompute_module_export_equals_non_module(&mut self) {
        self.module_export_equals_non_module.clear();
        for (module_name, exports) in self.module_exports.clone() {
            if let Some(non_module) = self.compute_module_export_equals_non_module(&exports) {
                self.module_export_equals_non_module
                    .insert(module_name, non_module);
            }
        }
    }

    /// Merge lib file symbols into the current scope.
    ///
    /// This is called during binder initialization to ensure global symbols
    /// from lib.d.ts (like `Object`, `Function`, `console`, etc.) are available
    /// during type checking.
    ///
    /// This method now uses `merge_lib_contexts_into_binder` which properly
    /// remaps `SymbolIds` to avoid collisions across lib binders.
    ///
    /// # Parameters
    /// - `lib_files`: Slice of Arc<LibFile> containing parsed and bound lib files
    ///
    /// # Example
    /// ```ignore
    /// let mut binder = BinderState::new();
    /// binder.bind_source_file(arena, root);
    /// binder.merge_lib_symbols(&lib_files);
    /// ```
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn merge_lib_symbols(&mut self, lib_files: &[Arc<lib_loader::LibFile>]) {
        // Merging lib globals changes visible symbols, so invalidate identifier cache.
        self.resolved_identifier_cache.write().unwrap().clear();

        // Convert LibFiles to LibContexts
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();

        // Use the new merge helper that properly remaps SymbolIds
        self.merge_lib_contexts_into_binder(&lib_contexts);

        // Also merge into the current scope if we're at the root level
        if self.scope_chain.len() <= 1 {
            for (name, sym_id) in self.file_locals.iter() {
                if !self.current_scope.has(name) {
                    self.current_scope.set(name.clone(), *sym_id);
                }
            }
        }

        // Merge into the root persistent scope
        if let Some(root_scope) = self.scopes.first_mut() {
            for (name, sym_id) in self.file_locals.iter() {
                if !root_scope.table.has(name) {
                    root_scope.table.set(name.clone(), *sym_id);
                }
            }
        }

        // Note: We no longer need to track lib_binders separately since
        // all lib symbols are now in our local symbol arena with unique IDs.
        // However, we keep lib_binders populated for backward compatibility
        // with any code that still iterates through them.
        for lib in lib_files {
            self.lib_binders.push(Arc::clone(&lib.binder));
        }
    }

    /// Bind a source file with lib symbols merged in.
    ///
    /// This is a convenience method that combines `bind_source_file` and `merge_lib_symbols`.
    ///
    /// CRITICAL: Lib symbols MUST be merged BEFORE binding the source file so that
    /// global symbols like `console`, `Array`, `Promise` are available during binding.
    /// If we bind first, the binder will emit TS2304 errors for these symbols.
    ///
    /// # Parameters
    /// - `arena`: The `NodeArena` containing the AST
    /// - `root`: The root node index of the source file
    /// - `lib_files`: Optional slice of Arc<LibFile> containing lib files
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn bind_source_file_with_libs(
        &mut self,
        arena: &NodeArena,
        root: NodeIndex,
        lib_files: &[Arc<lib_loader::LibFile>],
    ) {
        // IMPORTANT: Merge lib symbols FIRST so they're available during binding
        if !lib_files.is_empty() {
            self.merge_lib_symbols(lib_files);
        }
        self.bind_source_file(arena, root);
    }

    /// Incrementally bind new statements after a prefix without rebinding the entire file.
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn bind_source_file_incremental(
        &mut self,
        arena: &NodeArena,
        root: NodeIndex,
        prefix_statements: &[NodeIndex],
        old_suffix_statements: &[NodeIndex],
        new_suffix_statements: &[NodeIndex],
        reparse_start: u32,
    ) -> bool {
        // Incremental binding mutates scopes; clear stale identifier resolutions.
        self.resolved_identifier_cache.write().unwrap().clear();

        let last_prefix = match prefix_statements.last() {
            Some(stmt) => *stmt,
            None => return false,
        };
        let start_flow = match self.top_level_flow.get(&last_prefix.0) {
            Some(flow) => *flow,
            None => return false,
        };
        if self.scopes.is_empty() {
            return false;
        }

        self.is_external_module = Self::source_file_is_external_module(arena, root);

        // Detect strict mode for incremental rebinding
        if let Some(node) = arena.get(root)
            && let Some(sf) = arena.get_source_file(node)
        {
            self.is_strict_scope = self.options.always_strict
                || Self::has_use_strict_prologue(arena, &sf.statements.nodes);
        }

        self.prune_incremental_maps(arena, reparse_start);

        let mut prefix_names = FxHashSet::default();
        self.collect_file_scope_names_for_statements(arena, prefix_statements, &mut prefix_names);

        let mut old_suffix_names = FxHashSet::default();
        self.collect_file_scope_names_for_statements(
            arena,
            old_suffix_statements,
            &mut old_suffix_names,
        );

        for name in old_suffix_names {
            if prefix_names.contains(&name) {
                continue;
            }
            self.file_locals.remove(&name);
            if let Some(scope) = self.scopes.get_mut(0) {
                scope.table.remove(&name);
            }
        }

        let mut symbol_nodes = Vec::new();
        self.collect_statement_symbol_nodes(arena, old_suffix_statements, &mut symbol_nodes);
        for node in symbol_nodes {
            if let Some(sym_id) = self.node_symbols.remove(&node.0)
                && let Some(sym) = self.symbols.get_mut(sym_id)
            {
                sym.declarations.retain(|decl| *decl != node);
                if sym.value_declaration == node {
                    sym.value_declaration =
                        sym.declarations.first().copied().unwrap_or(NodeIndex::NONE);
                }
            }
        }

        for stmt_idx in old_suffix_statements {
            self.top_level_flow.remove(&stmt_idx.0);
        }

        // Reset transient binding state while keeping existing symbols and scopes.
        self.scope_chain.clear();
        self.scope_chain
            .push(ScopeContext::new(ContainerKind::SourceFile, root, None));
        self.current_scope_idx = 0;
        self.scope_stack.clear();
        self.current_scope = self.file_locals.clone();
        self.hoisted_vars.clear();
        self.hoisted_functions.clear();
        self.current_scope_id = ScopeId(0);
        self.current_flow = start_flow;

        let new_suffix_list = NodeList {
            nodes: new_suffix_statements.to_vec(),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        };

        self.collect_hoisted_declarations(arena, &new_suffix_list);
        self.process_hoisted_functions(arena);
        self.process_hoisted_vars(arena);

        for &stmt_idx in new_suffix_statements {
            self.bind_node(arena, stmt_idx);
            self.top_level_flow.insert(stmt_idx.0, self.current_flow);
        }

        self.sync_current_scope_to_persistent();

        // Store file locals, preserving any existing lib symbols
        // This ensures symbols from merge_lib_symbols() are not lost
        let existing_file_locals = std::mem::take(&mut self.file_locals);
        self.file_locals = std::mem::take(&mut self.current_scope);
        // Merge back any existing file locals (e.g., lib symbols) that were pre-populated
        for (name, sym_id) in existing_file_locals.iter() {
            if !self.file_locals.has(name) {
                self.file_locals.set(name.clone(), *sym_id);
            }
        }

        true
    }

    pub(crate) fn prune_incremental_maps(&mut self, arena: &NodeArena, reparse_start: u32) {
        if reparse_start == 0 {
            return;
        }

        let keep_node = |node_id: &u32| {
            arena
                .get(NodeIndex(*node_id))
                .is_some_and(|node| node.pos < reparse_start)
        };

        self.node_flow.retain(|node_id, _| keep_node(node_id));
        self.node_scope_ids.retain(|node_id, _| keep_node(node_id));
        self.switch_clause_to_switch
            .retain(|node_id, _| keep_node(node_id));
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
