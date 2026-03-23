//! Binder - Binder implementation using `NodeArena`.
//!
//! This is a clean implementation of the binder that works directly with
//! Node and `NodeArena`, avoiding the old Node enum pattern matching.

mod core;
mod flow_helpers;
mod lib_merge;
mod resolution;

use crate::modules::resolution_debug::ModuleResolutionDebugger;
use crate::{FlowNodeArena, FlowNodeId, Scope, ScopeId, SymbolArena, SymbolId, SymbolTable};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::RwLock;
use tsz_common::common::ScriptTarget;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Map from `(SymbolId, NodeIndex)` to the arena(s) containing that declaration.
/// Uses `SmallVec` to handle cross-arena `NodeIndex` collisions with zero overhead
/// for the common single-arena case.
pub type DeclarationArenaMap = FxHashMap<(SymbolId, NodeIndex), SmallVec<[Arc<NodeArena>; 1]>>;

/// Map from arena pointer (as `usize`) to that arena's `node_symbols` mapping.
/// Enables cross-file declaration resolution: when a symbol has declarations in
/// multiple arenas, the checker can look up the correct `node_symbols` for each
/// arena to resolve type references within cross-file interface declarations.
pub type CrossFileNodeSymbols = FxHashMap<usize, Arc<FxHashMap<u32, SymbolId>>>;

pub(crate) const MAX_SCOPE_WALK_ITERATIONS: usize = 10_000;

type ReexportTarget = (String, Option<String>);
type FileReexports = FxHashMap<String, ReexportTarget>;
type FileReexportsMap = FxHashMap<String, FileReexports>;
type ExportCache = FxHashMap<(String, String), Option<SymbolId>>;
type IdentifierCache = FxHashMap<(usize, u32), Option<SymbolId>>;
/// Wrapper around `RwLock` that implements `Clone` by cloning the inner data.
/// Used for caches that need thread-safety in parallel compilation but also
/// need to support `BinderState::clone()` for the checker lib context optimization.
#[derive(Debug, Default)]
pub(crate) struct CloneableRwLock<T>(RwLock<T>);

impl<T: Clone> Clone for CloneableRwLock<T> {
    fn clone(&self) -> Self {
        let inner = self.0.read().expect("RwLock poisoned during clone");
        Self(RwLock::new(inner.clone()))
    }
}

impl<T> std::ops::Deref for CloneableRwLock<T> {
    type Target = RwLock<T>;
    #[inline]
    fn deref(&self) -> &RwLock<T> {
        &self.0
    }
}

type ExportCacheStorage = CloneableRwLock<ExportCache>;
type IdentifierCacheStorage = CloneableRwLock<IdentifierCache>;

/// Bitflags tracking which language features are used in a source file.
///
/// Populated by the binder during its AST walk (zero-cost at check time).
/// The checker queries these to decide whether to emit TS2318 diagnostics
/// for missing global types like `IterableIterator`, `TypedPropertyDescriptor`, etc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug)]
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
    pub(crate) scope_chain: Vec<crate::ScopeContext>,
    /// Current scope index in `scope_chain`
    pub(crate) current_scope_idx: usize,
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: FxHashMap<u32, bool>,
    /// Symbol-to-arena mapping for cross-file declaration lookup (legacy, stores last arena)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (`SymbolId`, `NodeIndex` of declaration) -> Arena(s) containing that declaration
    /// This is needed when a symbol (like Array) is declared across multiple lib files.
    /// Uses `SmallVec` to handle cross-arena `NodeIndex` collisions: when two lib files have
    /// their interface declaration at the same `NodeIndex`, both arenas are stored.
    pub declaration_arenas: DeclarationArenaMap,
    /// Cross-file `node_symbols`: maps arena pointer → `node_symbols` for that arena.
    /// Enables resolving type references in cross-file interface declarations.
    pub cross_file_node_symbols: CrossFileNodeSymbols,
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

    /// Maps symbols declared inside `declare module "..."` augmentation blocks to their
    /// target module specifier. Used by the checker to redirect type resolution for
    /// self-referential augmentation interfaces (e.g., `interface Foo { self: Foo }` inside
    /// `declare module "./m"` should resolve Foo to the merged interface, not just the
    /// augmentation-local one).
    pub augmentation_target_modules: FxHashMap<SymbolId, String>,

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
    /// Tracks whether wildcard re-export entries are type-only.
    /// Maps `current_file` -> Vec of (`source_module`, `is_type_only`).
    /// This captures `export type * from './module'` chains during import resolution.
    pub wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>>,

    /// Cache for resolved exports to avoid repeated lookups through re-export chains.
    /// Key: (`module_specifier`, `export_name`) -> resolved `SymbolId` (or None if not found)
    /// This cache dramatically speeds up barrel file imports where the same export
    /// is looked up multiple times across different files.
    /// Uses `RwLock` for thread-safety in parallel compilation.
    pub(crate) resolved_export_cache: ExportCacheStorage,
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

    /// Continue targets for control flow analysis.
    /// When we enter a loop, we push the flow label that continue statements jump to.
    pub(crate) continue_targets: Vec<FlowNodeId>,

    /// Return targets for IIFE control flow analysis.
    /// When inside an IIFE body, return statements redirect flow to this label
    /// instead of making the outer flow unreachable. This implements tsc's behavior
    /// where non-async, non-generator IIFEs are part of the containing control flow.
    pub(crate) return_targets: Vec<FlowNodeId>,

    /// Language features detected during binding (generators, decorators, using, etc.).
    /// Populated during `bind_source_file` with zero overhead since the binder already walks every node.
    pub file_features: FileFeatures,

    /// Alias partners: maps a `TYPE_ALIAS` SymbolId to its ALIAS (namespace export) partner.
    /// When `export type X = ...` and `export * as X from "..."` coexist in the same module,
    /// the exports table holds the `TYPE_ALIAS` symbol (for type reference resolution) and this
    /// map links it to the ALIAS symbol (for value/namespace resolution).
    /// Populated by `merge_bind_results` in parallel.rs.
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,

    /// Module specifier strings from static import/export declarations.
    ///
    /// Collected during binding so consumers (LSP, CLI) can query a file's
    /// import sources without re-walking the AST. Contains the raw specifier
    /// text (e.g. `"./utils"`, `"react"`) from:
    /// - `import ... from "specifier"`
    /// - `import "specifier"` (side-effect import)
    /// - `export ... from "specifier"`
    /// - `export * from "specifier"`
    /// - `import X = require("specifier")`
    ///
    /// Does NOT include dynamic `import()` or `require()` call expressions,
    /// which require AST-level analysis.
    pub file_import_sources: Vec<String>,

    // ===== DefId-First Stable Identity (Phase 1) =====
    /// Binder-owned semantic definition index for top-level declarations.
    ///
    /// Maps `SymbolId` → `SemanticDefEntry` for CLASS, INTERFACE, `TYPE_ALIAS`, ENUM,
    /// and NAMESPACE/MODULE symbols declared at the top level. Populated during
    /// `declare_symbol` so the checker can pre-create solver `DefId`s before type
    /// checking begins, avoiding on-demand identity creation in hot checker paths.
    ///
    /// This is the binder's contribution to stable semantic identity (Phase 1).
    /// The checker converts these entries to solver `DefId`s during construction.
    pub semantic_defs: FxHashMap<SymbolId, SemanticDefEntry>,

    /// Stable file index assigned by the driver (LSP `Project` or CLI).
    ///
    /// Defaults to `u32::MAX` (unassigned). When set before `bind_source_file`,
    /// all symbols created during binding will have their `decl_file_idx` stamped
    /// with this value, and all `SemanticDefEntry.file_id` fields will use it.
    ///
    /// This enables per-file invalidation in the `DefinitionStore`: when a file
    /// is removed or replaced, the driver calls `invalidate_file(file_idx)` to
    /// clean up all definitions registered under that index.
    pub file_idx: u32,
}

/// Kind of semantic definition captured at bind time.
///
/// Mirrors `tsz_solver::def::DefKind` but lives in the binder crate to avoid
/// a circular dependency (solver depends on binder). The checker converts these
/// to solver `DefKind` during `DefId` pre-population.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticDefKind {
    /// Type alias: `type Foo = number`
    TypeAlias,
    /// Interface: `interface Point { x: number }`
    Interface,
    /// Class: `class Foo {}`
    Class,
    /// Enum: `enum Color { Red, Green }`
    Enum,
    /// Namespace or module: `namespace NS {}` or `module M {}`
    Namespace,
    /// Function declaration: `function foo() {}`
    Function,
    /// Variable declaration: `const x = 1` or `let y: string`
    Variable,
}

/// Binder-captured semantic identity for a top-level declaration.
///
/// Contains exactly the information needed for the checker to create a solver
/// `DefId` + `DefinitionInfo` without re-examining the AST or symbol table.
/// This is populated during binding and consumed during checker construction.
#[derive(Clone, Debug)]
pub struct SemanticDefEntry {
    /// What kind of declaration this is.
    pub kind: SemanticDefKind,
    /// The escaped name of the declaration.
    pub name: String,
    /// File index for this declaration (from `Symbol.decl_file_idx`).
    pub file_id: u32,
    /// Start position of the first declaration (for content-addressed stability).
    pub span_start: u32,
    /// Number of type parameters on the declaration (0 for non-generic).
    ///
    /// Captured at bind time from `type_parameters.nodes.len()` for interfaces,
    /// classes, type aliases, and functions. This allows the checker's DefId
    /// pre-population to create `DefinitionInfo` with the correct type parameter
    /// arity, enabling the `TypeFormatter` to display generic types with
    /// placeholder parameters (e.g., `Map<unknown, unknown>`) before the full
    /// checker walk fills in the real `TypeParamInfo`.
    pub type_param_count: u16,
    /// Names of type parameters on the declaration (empty for non-generic).
    ///
    /// Captured at bind time from the identifier names of each type parameter
    /// node. For example, `class Foo<T, U>` yields `["T", "U"]`. This allows
    /// the pre-populated `DefinitionInfo` to have `TypeParamInfo` with real
    /// names instead of `Atom(0)` stubs, improving diagnostic and formatting
    /// quality before the full checker walk fills in constraints/defaults.
    ///
    /// Invariant: `type_param_names.len() == type_param_count as usize`.
    pub type_param_names: Vec<String>,
    /// Whether the declaration has an `export` modifier or is otherwise exported.
    ///
    /// Captured at bind time so the checker and file-skeleton infrastructure can
    /// determine export visibility without re-examining the symbol table. This is
    /// a prerequisite for Phase 2 file-skeleton decomposition where export surfaces
    /// are extracted from binder-owned identity rather than full symbol residency.
    pub is_exported: bool,
    /// Enum member names (empty for non-enum declarations).
    ///
    /// Captured at bind time so the checker's DefId pre-population can create
    /// `DefinitionInfo` with stub enum members. This avoids the checker needing
    /// to walk enum member declarations on demand to populate the definition.
    pub enum_member_names: Vec<String>,
    /// Whether this is a `const enum` declaration.
    ///
    /// Only meaningful for `SemanticDefKind::Enum`; always `false` for other kinds.
    pub is_const: bool,
    /// Whether this is an `abstract class` declaration.
    ///
    /// Only meaningful for `SemanticDefKind::Class`; always `false` for other kinds.
    pub is_abstract: bool,
    /// Names referenced in `extends` heritage clauses.
    ///
    /// Captured at bind time from `extends` clause expressions of class and
    /// interface declarations. For example, `class Foo extends Bar` yields
    /// `["Bar"]` and `interface A extends B, C` yields `["B", "C"]`.
    ///
    /// Only simple identifier names are captured; property-access heritage
    /// expressions (e.g., `ns.Base`) are stored as dot-separated strings.
    ///
    /// Used by pre-population to wire `DefinitionInfo.extends` at identity
    /// creation time, moving class hierarchy identity from checker-side type
    /// resolution to binder-owned stable identity.
    pub extends_names: Vec<String>,
    /// Names referenced in `implements` heritage clauses.
    ///
    /// Captured at bind time from `implements` clause expressions of class
    /// declarations. For example, `class Foo implements IBar, IBaz` yields
    /// `["IBar", "IBaz"]`. Interfaces do not have `implements` clauses.
    ///
    /// Used by pre-population to wire `DefinitionInfo.implements` at identity
    /// creation time.
    pub implements_names: Vec<String>,
    /// The `SymbolId` of the containing namespace/module, if this declaration
    /// lives inside one.
    ///
    /// Captured at bind time when the declaration is in a `ContainerKind::Module`
    /// scope (but not the source-file root scope). During merge, this is remapped
    /// to the global `SymbolId`. During `pre_populate_definition_store`, this is
    /// used to wire up `DefinitionInfo.exports` so namespace members have stable
    /// export identity without checker-side repair.
    ///
    /// `None` for top-level (source-file scope) declarations.
    pub parent_namespace: Option<SymbolId>,
    /// Whether this declaration was captured inside a `declare global { }` block.
    ///
    /// Declarations inside `declare global` blocks are semantically global
    /// augmentations — they merge with lib.d.ts symbols at type resolution time.
    /// This flag allows the merge pipeline and pre-population to distinguish
    /// true top-level declarations from global augmentations, enabling correct
    /// identity resolution when augmented types need special handling (e.g.,
    /// cross-batch heritage resolution for `declare global { interface Array<T> { ... } }`).
    pub is_global_augmentation: bool,

    /// Whether this declaration has the `declare` modifier or is in an ambient
    /// context (`.d.ts` file).
    ///
    /// Captured at bind time so that the checker and solver can distinguish
    /// ambient declarations from implementation declarations without
    /// re-examining the AST or symbol modifiers. Ambient declarations have
    /// no runtime representation; this flag enables the checker to suppress
    /// certain diagnostics and gate emit behaviour.
    ///
    /// Propagated through merge and pre-population to `DefinitionInfo.is_declare`.
    pub is_declare: bool,
}

impl SemanticDefEntry {
    /// Combined heritage names (extends + implements) for fingerprinting.
    ///
    /// Returns a combined view of `extends_names` and `implements_names` for
    /// backward compatibility with code that uses the combined heritage list
    /// (e.g., `BinderFileSummary` fingerprinting).
    pub fn heritage_names(&self) -> Vec<String> {
        let mut combined = self.extends_names.clone();
        combined.extend(self.implements_names.iter().cloned());
        combined
    }

    /// Accumulate metadata from a cross-file declaration merge into this entry.
    ///
    /// When the same symbol appears in multiple files (e.g., cross-file interface
    /// merging, or split enum declarations), the first file's entry is kept as the
    /// canonical identity but subsequent files may contribute additional heritage
    /// names, enum members, export visibility, and type parameter arity.
    ///
    /// This mirrors the within-file accumulation logic in `record_semantic_def_ext`
    /// but runs during the merge phase in `parallel/core.rs`.
    pub fn merge_cross_file(&mut self, other: &SemanticDefEntry) {
        // Accumulate extends names not already present.
        for h in &other.extends_names {
            if !self.extends_names.contains(h) {
                self.extends_names.push(h.clone());
            }
        }
        // Accumulate implements names not already present.
        for h in &other.implements_names {
            if !self.implements_names.contains(h) {
                self.implements_names.push(h.clone());
            }
        }
        // If the first declaration had no type params but a later file does
        // (e.g., augmentation adds generics), update the arity and names.
        if self.type_param_count == 0 && other.type_param_count > 0 {
            self.type_param_count = other.type_param_count;
            self.type_param_names = other.type_param_names.clone();
        }
        // If the later declaration is exported, mark as exported.
        if other.is_exported {
            self.is_exported = true;
        }
        // Accumulate enum members from later declarations.
        for m in &other.enum_member_names {
            if !self.enum_member_names.contains(m) {
                self.enum_member_names.push(m.clone());
            }
        }
        // Promote abstract flag if any declaration is abstract.
        if other.is_abstract {
            self.is_abstract = true;
        }
        // Promote const flag if any declaration is const (for enums).
        if other.is_const {
            self.is_const = true;
        }
        // Promote global augmentation flag if any declaration is from declare global.
        if other.is_global_augmentation {
            self.is_global_augmentation = true;
        }
        // Promote declare flag if any declaration has the declare modifier.
        if other.is_declare {
            self.is_declare = true;
        }
    }
}

// =============================================================================
// BinderFileSummary - Lightweight file summary for dependency graphs
// (test-only: used to verify binder captures exports/heritage correctly;
//  the production skeleton is `tsz_core::parallel::skeleton::FileSkeleton`)
// =============================================================================

#[cfg(test)]
mod file_summary {
    use super::*;

    /// A lightweight summary of a file's type surface, extracted from `BinderState`.
    ///
    /// This is a binder-internal type used for testing that semantic definitions,
    /// exports, and heritage dependencies are captured correctly. For the
    /// production skeleton used in the parallel pipeline (merge topology,
    /// incremental invalidation), see `tsz_core::parallel::skeleton::FileSkeleton`.
    #[derive(Clone, Debug, PartialEq)]
    pub(crate) struct BinderFileSummary {
        /// Stable file index (same as `BinderState.file_idx`).
        pub file_idx: u32,
        /// Module specifiers from static import/export declarations.
        pub import_sources: Vec<String>,
        /// Exported declarations: `(name, kind)` pairs.
        pub exported_defs: Vec<BinderExportEntry>,
        /// Names referenced in heritage clauses across all declarations.
        pub heritage_deps: Vec<String>,
        /// Whether this file is an external module (has import/export syntax).
        pub is_external_module: bool,
    }

    /// An exported declaration in a `BinderFileSummary`.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub(crate) struct BinderExportEntry {
        /// Name of the exported declaration.
        pub name: String,
        /// Kind of declaration (class, interface, type alias, etc.).
        pub kind: SemanticDefKind,
        /// Number of type parameters (for generic signature matching).
        pub type_param_count: u16,
    }

    impl BinderFileSummary {
        /// Returns all module specifiers this file depends on (deduplicated).
        pub fn dependency_specifiers(&self) -> Vec<&str> {
            let mut specs: Vec<&str> = self.import_sources.iter().map(|s| s.as_str()).collect();
            let mut seen = FxHashSet::default();
            specs.retain(|s| seen.insert(*s));
            specs
        }

        /// Returns `true` if the file exports any declarations.
        pub const fn has_exports(&self) -> bool {
            !self.exported_defs.is_empty()
        }

        /// Returns `true` if the file has any heritage dependencies.
        pub const fn has_heritage_deps(&self) -> bool {
            !self.heritage_deps.is_empty()
        }

        /// Compute a simple fingerprint of the file's public API surface.
        pub fn api_fingerprint(&self) -> u64 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for exp in &self.exported_defs {
                exp.hash(&mut hasher);
            }
            for dep in &self.heritage_deps {
                dep.hash(&mut hasher);
            }
            hasher.finish()
        }
    }

    impl BinderState {
        /// Extract a `BinderFileSummary` summarizing this file's type surface.
        ///
        /// This is a lightweight operation that reads from already-populated
        /// binder state fields. Call after `bind_source_file` completes.
        pub(crate) fn file_summary(&self) -> BinderFileSummary {
            let mut exported_defs = Vec::new();
            let mut heritage_deps_set = FxHashSet::default();

            for entry in self.semantic_defs.values() {
                for h in &entry.extends_names {
                    heritage_deps_set.insert(h.clone());
                }
                for h in &entry.implements_names {
                    heritage_deps_set.insert(h.clone());
                }
                if entry.is_exported {
                    exported_defs.push(BinderExportEntry {
                        name: entry.name.clone(),
                        kind: entry.kind,
                        type_param_count: entry.type_param_count,
                    });
                }
            }

            exported_defs.sort_by(|a, b| a.name.cmp(&b.name));
            let mut heritage_deps: Vec<String> = heritage_deps_set.into_iter().collect();
            heritage_deps.sort();

            BinderFileSummary {
                file_idx: self.file_idx,
                import_sources: self.file_import_sources.clone(),
                exported_defs,
                heritage_deps,
                is_external_module: self.is_external_module,
            }
        }
    }
}

impl BinderState {
    /// Clear resolution caches that were populated during binding.
    /// Called after cloning a binder for the checker, which needs a clean
    /// cache state for its own symbol resolution.
    pub fn clear_resolution_caches(&mut self) {
        self.resolved_export_cache
            .write()
            .expect("not poisoned")
            .clear();
        self.resolved_identifier_cache
            .write()
            .expect("not poisoned")
            .clear();
    }
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
    pub augmentation_target_modules: FxHashMap<SymbolId, String>,
    pub module_exports: FxHashMap<String, SymbolTable>,
    pub module_declaration_exports_publicly: FxHashMap<u32, bool>,
    pub reexports: FileReexportsMap,
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    pub wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>>,
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    pub declaration_arenas: DeclarationArenaMap,
    pub cross_file_node_symbols: CrossFileNodeSymbols,
    pub shorthand_ambient_modules: FxHashSet<String>,
    pub modules_with_export_equals: FxHashSet<String>,
    pub flow_nodes: FlowNodeArena,
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    pub switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,
}

#[cfg(test)]
mod tests;
