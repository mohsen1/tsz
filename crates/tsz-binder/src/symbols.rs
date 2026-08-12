//! Symbol types, flags, and arena for the binder.
//!
//! Provides `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`, and `symbol_flags`.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsz_common::define_id;
use tsz_parser::NodeIndex;

// =============================================================================
// Symbol Flags
// =============================================================================

/// Flags that describe the kind and properties of a symbol.
/// Matches TypeScript's `SymbolFlags` enum in src/compiler/types.ts
pub mod symbol_flags {
    pub const NONE: u32 = 0;
    pub const FUNCTION_SCOPED_VARIABLE: u32 = 1 << 0; // Variable (var) or parameter
    pub const BLOCK_SCOPED_VARIABLE: u32 = 1 << 1; // Block-scoped variable (let or const)
    pub const PROPERTY: u32 = 1 << 2; // Property or enum member
    pub const ENUM_MEMBER: u32 = 1 << 3; // Enum member
    pub const FUNCTION: u32 = 1 << 4; // Function
    pub const CLASS: u32 = 1 << 5; // Class
    pub const INTERFACE: u32 = 1 << 6; // Interface
    pub const CONST_ENUM: u32 = 1 << 7; // Const enum
    pub const REGULAR_ENUM: u32 = 1 << 8; // Enum
    pub const VALUE_MODULE: u32 = 1 << 9; // Instantiated module
    pub const NAMESPACE_MODULE: u32 = 1 << 10; // Uninstantiated module
    pub const TYPE_LITERAL: u32 = 1 << 11; // Type Literal or mapped type
    pub const OBJECT_LITERAL: u32 = 1 << 12; // Object Literal
    pub const METHOD: u32 = 1 << 13; // Method
    pub const CONSTRUCTOR: u32 = 1 << 14; // Constructor
    pub const GET_ACCESSOR: u32 = 1 << 15; // Get accessor
    pub const SET_ACCESSOR: u32 = 1 << 16; // Set accessor
    pub const SIGNATURE: u32 = 1 << 17; // Call, construct, or index signature
    pub const TYPE_PARAMETER: u32 = 1 << 18; // Type parameter
    pub const TYPE_ALIAS: u32 = 1 << 19; // Type alias
    pub const EXPORT_VALUE: u32 = 1 << 20; // Exported value marker
    pub const ALIAS: u32 = 1 << 21; // Alias for another symbol
    pub const PROTOTYPE: u32 = 1 << 22; // Prototype property
    pub const EXPORT_STAR: u32 = 1 << 23; // Export * declaration
    pub const OPTIONAL: u32 = 1 << 24; // Optional property
    pub const TRANSIENT: u32 = 1 << 25; // Transient symbol
    pub const ASSIGNMENT: u32 = 1 << 26; // Assignment treated as declaration
    pub const MODULE_EXPORTS: u32 = 1 << 27; // CommonJS module.exports
    pub const PRIVATE: u32 = 1 << 28; // Private member
    pub const PROTECTED: u32 = 1 << 29; // Protected member
    pub const ABSTRACT: u32 = 1 << 30; // Abstract member
    pub const STATIC: u32 = 1 << 31; // Static member

    // Composite flags
    pub const ENUM: u32 = REGULAR_ENUM | CONST_ENUM;
    pub const VARIABLE: u32 = FUNCTION_SCOPED_VARIABLE | BLOCK_SCOPED_VARIABLE;
    pub const VALUE: u32 = VARIABLE
        | PROPERTY
        | ENUM_MEMBER
        | OBJECT_LITERAL
        | FUNCTION
        | CLASS
        | ENUM
        | VALUE_MODULE
        | METHOD
        | GET_ACCESSOR
        | SET_ACCESSOR;
    pub const TYPE: u32 =
        CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS;
    pub const NAMESPACE: u32 = VALUE_MODULE | NAMESPACE_MODULE | ENUM;
    pub const MODULE: u32 = VALUE_MODULE | NAMESPACE_MODULE;
    pub const ACCESSOR: u32 = GET_ACCESSOR | SET_ACCESSOR;

    // Exclusion flags for redeclaration checks
    // Note: Operator precedence in Rust has & binding tighter than |, so we need parentheses
    // to match TypeScript's semantics for declaration merging rules.
    pub const FUNCTION_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE & !FUNCTION_SCOPED_VARIABLE;
    pub const BLOCK_SCOPED_VARIABLE_EXCLUDES: u32 = VALUE;
    pub const PARAMETER_EXCLUDES: u32 = VALUE;
    pub const PROPERTY_EXCLUDES: u32 = NONE;
    pub const ENUM_MEMBER_EXCLUDES: u32 = VALUE | TYPE;
    // Function can merge with: namespace/module (VALUE_MODULE) and class
    pub const FUNCTION_EXCLUDES: u32 = VALUE & !FUNCTION & !VALUE_MODULE & !CLASS;
    // Class can merge with: interface, function, and namespace/module
    pub const CLASS_EXCLUDES: u32 = (VALUE | TYPE) & !VALUE_MODULE & !INTERFACE & !FUNCTION;
    // Interface can merge with: interface, class
    pub const INTERFACE_EXCLUDES: u32 = TYPE & !INTERFACE & !CLASS;
    // Enum can merge with: namespace/module and same-kind enum
    pub const REGULAR_ENUM_EXCLUDES: u32 = (VALUE | TYPE) & !REGULAR_ENUM & !VALUE_MODULE;
    pub const CONST_ENUM_EXCLUDES: u32 = (VALUE | TYPE) & !CONST_ENUM & !VALUE_MODULE;
    // Value module (namespace with values) can merge with: function, class, enum, and other value modules
    pub const VALUE_MODULE_EXCLUDES: u32 =
        VALUE & !FUNCTION & !CLASS & !REGULAR_ENUM & !VALUE_MODULE;
    // Pure namespace module can merge with anything
    pub const NAMESPACE_MODULE_EXCLUDES: u32 = NONE;
    pub const METHOD_EXCLUDES: u32 = VALUE & !METHOD;
    pub const GET_ACCESSOR_EXCLUDES: u32 = VALUE & !SET_ACCESSOR;
    pub const SET_ACCESSOR_EXCLUDES: u32 = VALUE & !GET_ACCESSOR;
    pub const TYPE_PARAMETER_EXCLUDES: u32 = TYPE & !TYPE_PARAMETER;
    pub const TYPE_ALIAS_EXCLUDES: u32 = TYPE;
    pub const ALIAS_EXCLUDES: u32 = ALIAS;
}

// =============================================================================
// Stable Location
// =============================================================================

/// A file-stable pointer to an AST declaration.
///
/// `StableLocation` identifies a declaration *without* requiring its owning
/// `NodeArena` to be resident in memory. It combines a stable driver-assigned
/// file index with the source span `(pos, end)` of the declaration node. A
/// re-parse of the same file will produce the same span, so
/// `StableLocation`s survive arena drop/rehydrate cycles — unlike the raw
/// `NodeIndex` values currently stored on `Symbol`, whose meaning depends on
/// the exact arena that produced them.
///
/// This is the Phase 1 foundation for the
/// [global query graph architecture][plan]: it lets future work resolve
/// symbols, `DefId`s, and cross-file references by `(file_idx, span)` pairs
/// instead of cloned `Arc<NodeArena>` handles. Consumers continue to use the
/// parallel `NodeIndex` fields today; migrating them is handled by follow-up
/// PRs.
///
/// Size: `#[repr(C)]` with three `u32` fields is 12 bytes and `Copy`.
///
/// ## Invariants
/// - `file_idx == u32::MAX` indicates "unassigned". Single-file binding paths
///   populate stable locations with `file_idx = u32::MAX`; the driver later
///   stamps the concrete file index via
///   [`BinderState::stamp_file_idx`][stamp].
/// - When both `pos` and `end` are `0`, the stable location is
///   unavailable/unknown and should be treated as `None` by consumers. Use
///   [`StableLocation::is_known`] to distinguish.
/// - `pos <= end` is expected for any known location.
///
/// [plan]: ../../../docs/plan/ROADMAP.md
/// [stamp]: crate::state::BinderState::stamp_file_idx
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableLocation {
    /// Driver-assigned file index. `u32::MAX` means "not yet stamped".
    pub file_idx: u32,
    /// Byte offset of the declaration's start in the source file.
    pub pos: u32,
    /// Byte offset of the declaration's end (exclusive) in the source file.
    pub end: u32,
}

impl StableLocation {
    /// Sentinel value representing an unknown/unset stable location.
    pub const NONE: Self = Self {
        file_idx: u32::MAX,
        pos: 0,
        end: 0,
    };

    /// Construct a stable location from a concrete file index and span.
    #[inline]
    #[must_use]
    pub const fn new(file_idx: u32, pos: u32, end: u32) -> Self {
        Self { file_idx, pos, end }
    }

    /// Construct a stable location with an unassigned file index.
    /// The binder uses this shape during single-file binding and defers
    /// file-index assignment to [`crate::state::BinderState::stamp_file_idx`].
    #[inline]
    #[must_use]
    pub const fn with_unassigned_file(pos: u32, end: u32) -> Self {
        Self {
            file_idx: u32::MAX,
            pos,
            end,
        }
    }

    /// Construct a stable location from an optional span, preserving the
    /// `NONE` sentinel when the span is unavailable.
    #[inline]
    #[must_use]
    pub const fn from_span(file_idx: u32, span: Option<(u32, u32)>) -> Self {
        match span {
            Some((pos, end)) => Self { file_idx, pos, end },
            None => Self::NONE,
        }
    }

    /// True when the location has been populated with a real source span.
    /// A `StableLocation` with `pos == 0 && end == 0` is treated as unknown.
    #[inline]
    #[must_use]
    pub const fn is_known(&self) -> bool {
        self.pos != 0 || self.end != 0
    }

    /// True when the file index has been stamped by the driver.
    #[inline]
    #[must_use]
    pub const fn has_file_idx(&self) -> bool {
        self.file_idx != u32::MAX
    }

    /// Stamp the file index if it is currently unassigned. No-op otherwise.
    /// Used by [`crate::state::BinderState::stamp_file_idx`] to finalize
    /// stable locations after the driver has assigned a file index.
    #[inline]
    pub const fn set_file_idx_if_unassigned(&mut self, file_idx: u32) {
        if self.file_idx == u32::MAX {
            self.file_idx = file_idx;
        }
    }
}

impl Default for StableLocation {
    fn default() -> Self {
        Self::NONE
    }
}

// =============================================================================
// Symbol
// =============================================================================

define_id! {
    /// Unique identifier for a symbol in the symbol table.
    pub struct SymbolId;
    derive: PartialOrd, Ord, Serialize, Deserialize;
    sentinel: max
}

/// A symbol represents a named entity in the program.
/// Symbols are created during binding and used during type checking.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol flags describing kind and properties
    pub flags: u32,
    /// Escaped name of the symbol
    pub escaped_name: String,
    /// Declarations associated with this symbol
    pub declarations: Vec<NodeIndex>,
    /// File-stable locations parallel to [`Self::declarations`].
    ///
    /// Each entry is a `(file_idx, pos, end)` triple that survives arena
    /// drop/rehydrate. This is the Phase 1 plumbing for the
    /// [global query graph architecture][plan]; consumers still read
    /// `declarations` (of `NodeIndex`) today. Populated in lockstep with
    /// `declarations` at every binding site, so `stable_declarations.len()
    /// == declarations.len()` is a hard invariant.
    ///
    /// [plan]: ../../../docs/plan/ROADMAP.md
    pub stable_declarations: Vec<StableLocation>,
    /// First value declaration of the symbol
    pub value_declaration: NodeIndex,
    /// File-stable location parallel to [`Self::value_declaration`].
    ///
    /// Phase 1 plumbing for re-parse-safe identity. Populated whenever
    /// `value_declaration` is set. Defaults to [`StableLocation::NONE`] when
    /// no value declaration has been recorded.
    pub stable_value_declaration: StableLocation,
    /// Parent symbol (for nested symbols)
    pub parent: SymbolId,
    /// Unique ID for this symbol
    pub id: SymbolId,
    /// Exported members for modules/namespaces
    pub exports: Option<Box<SymbolTable>>,
    /// Members for classes/interfaces
    pub members: Option<Box<SymbolTable>>,
    /// Whether this symbol is exported from its container (namespace/module)
    pub is_exported: bool,
    /// Whether this symbol is type-only (e.g., `import type`).
    pub is_type_only: bool,
    /// File index for cross-file resolution (set during multi-file merge)
    /// This indicates which file's arena contains this symbol's declarations.
    /// Value of `u32::MAX` means single-file mode (use current arena).
    pub decl_file_idx: u32,
    /// Out-of-lined import-alias payload.
    ///
    /// Fewer than ~5% of symbols are import aliases, yet the module specifier,
    /// renamed export name, and explicit `resolution-mode` override are
    /// per-import data. Storing them inline taxed every `Symbol` (millions per
    /// large project) with ~50 unused bytes. They now live behind a heap
    /// allocation that only import-alias symbols pay for (#13072 PR 2). Use the
    /// [`Symbol::import_module`], [`Symbol::import_name`], and
    /// [`Symbol::import_resolution_mode`] accessors to read, and
    /// [`Symbol::set_import_module`], [`Symbol::set_import_name`], and
    /// [`Symbol::set_import_resolution_mode`] to populate.
    pub import_alias: Option<Box<ImportAliasData>>,
    /// Whether this symbol is a UMD namespace export (`export as namespace Foo`).
    /// UMD exports are ALIAS symbols that should be globally visible across files,
    /// unlike regular import aliases which are file-local.
    pub is_umd_export: bool,
}

/// Out-of-lined import-alias payload for [`Symbol`].
///
/// Only import-alias symbols allocate this box, so the common `Symbol` stays
/// small (#13072). The checker and LSP read these through the accessors on
/// [`Symbol`]; nothing constructs an `ImportAliasData` directly outside the
/// `Symbol` setters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ImportAliasData {
    /// Import module specifier for ES6 imports (e.g., './file' for `import { X } from './file'`)
    /// This enables resolving imported symbols to their actual exports from other files.
    pub import_module: Option<String>,
    /// Original export name for imports with renamed imports (e.g., 'foo' for `import { foo as bar }`)
    /// If None, the import name matches the `escaped_name`.
    pub import_name: Option<String>,
    /// Explicit `resolution-mode` override for this import alias, when one was
    /// declared via an import attribute (`with { "resolution-mode": ... }`).
    ///
    /// Currently set for JSDoc `@import` aliases that carry an attribute clause.
    /// `None` means the specifier resolves through the importing file's own
    /// inferred module mode. The checker maps this onto the resolution request
    /// so package `exports`/`imports` conditions pick the right target file.
    pub import_resolution_mode: Option<tsz_common::ImportResolutionMode>,
}

impl Symbol {
    /// Create a new symbol with the given flags and name.
    #[must_use]
    pub const fn new(id: SymbolId, flags: u32, name: String) -> Self {
        Self {
            flags,
            escaped_name: name,
            declarations: Vec::new(),
            stable_declarations: Vec::new(),
            value_declaration: NodeIndex::NONE,
            stable_value_declaration: StableLocation::NONE,
            parent: SymbolId::NONE,
            id,
            exports: None,
            members: None,
            is_exported: false,
            is_type_only: false,
            decl_file_idx: u32::MAX,
            import_alias: None,
            is_umd_export: false,
        }
    }

    /// Import module specifier for ES6 imports (e.g., `./file` for
    /// `import { X } from './file'`), if this symbol is an import alias.
    ///
    /// Reads through the out-of-lined [`Self::import_alias`] payload (#13072).
    #[inline]
    #[must_use]
    pub fn import_module(&self) -> Option<&str> {
        self.import_alias
            .as_ref()
            .and_then(|data| data.import_module.as_deref())
    }

    /// Original export name for renamed imports (e.g., `foo` for
    /// `import { foo as bar }`), if this symbol is an import alias.
    ///
    /// `None` means the import name matches [`Self::escaped_name`]. Reads
    /// through the out-of-lined [`Self::import_alias`] payload (#13072).
    #[inline]
    #[must_use]
    pub fn import_name(&self) -> Option<&str> {
        self.import_alias
            .as_ref()
            .and_then(|data| data.import_name.as_deref())
    }

    /// Whether this is a *namespace-style alias* — an `import * as NS` or
    /// `export * as NS from "..."` binding: an alias (`symbol_flags::ALIAS`)
    /// with a module specifier and an import name of `"*"`. Such a symbol has
    /// no `NAMESPACE_MODULE` flag of its own, but acts as a namespace anchor
    /// whose members are the re-exported module's exports.
    #[inline]
    #[must_use]
    pub fn is_namespace_style_alias(&self) -> bool {
        self.has_any_flags(symbol_flags::ALIAS)
            && self.import_module().is_some()
            && self.import_name() == Some("*")
    }

    /// Explicit `resolution-mode` override declared on this import alias, if any.
    ///
    /// Reads through the out-of-lined [`Self::import_alias`] payload (#13072).
    #[inline]
    #[must_use]
    pub fn import_resolution_mode(&self) -> Option<tsz_common::ImportResolutionMode> {
        self.import_alias
            .as_ref()
            .and_then(|data| data.import_resolution_mode)
    }

    /// True when this symbol carries an import module specifier.
    #[inline]
    #[must_use]
    pub fn has_import_module(&self) -> bool {
        self.import_module().is_some()
    }

    /// Mutable handle to the out-of-lined import-alias payload, allocating it
    /// on first use. Only import-alias symbols ever allocate the box (#13072).
    #[inline]
    fn import_alias_mut(&mut self) -> &mut ImportAliasData {
        self.import_alias.get_or_insert_with(Box::default)
    }

    /// Set the import module specifier, allocating the import-alias payload if
    /// needed.
    #[inline]
    pub fn set_import_module(&mut self, module: Option<String>) {
        if module.is_none() && self.import_alias.is_none() {
            return;
        }
        self.import_alias_mut().import_module = module;
    }

    /// Set the renamed-import original name, allocating the import-alias payload
    /// if needed.
    #[inline]
    pub fn set_import_name(&mut self, name: Option<String>) {
        if name.is_none() && self.import_alias.is_none() {
            return;
        }
        self.import_alias_mut().import_name = name;
    }

    /// Set the explicit `resolution-mode` override, allocating the import-alias
    /// payload if needed.
    #[inline]
    pub fn set_import_resolution_mode(&mut self, mode: Option<tsz_common::ImportResolutionMode>) {
        if mode.is_none() && self.import_alias.is_none() {
            return;
        }
        self.import_alias_mut().import_resolution_mode = mode;
    }

    /// Estimate the heap bytes owned by this symbol beyond
    /// `size_of::<Symbol>()` (name, declaration lists, member tables).
    ///
    /// Capacity-based estimate for residency accounting (#13249 step 1);
    /// called only at perf-counter snapshot time, never on a hot path.
    #[must_use]
    pub fn estimated_heap_bytes(&self) -> usize {
        let mut size = self.escaped_name.capacity();
        size += self.declarations.capacity() * std::mem::size_of::<NodeIndex>();
        size += self.stable_declarations.capacity() * std::mem::size_of::<StableLocation>();
        if let Some(exports) = &self.exports {
            size += std::mem::size_of::<SymbolTable>() + exports.estimated_size_bytes();
        }
        if let Some(members) = &self.members {
            size += std::mem::size_of::<SymbolTable>() + members.estimated_size_bytes();
        }
        if let Some(alias) = &self.import_alias {
            size += std::mem::size_of::<ImportAliasData>();
            if let Some(module) = &alias.import_module {
                size += module.capacity();
            }
            if let Some(name) = &alias.import_name {
                size += name.capacity();
            }
        }
        size
    }

    /// Check if symbol has all specified flags.
    #[must_use]
    pub const fn has_flags(&self, flags: u32) -> bool {
        (self.flags & flags) == flags
    }

    /// Check if symbol has any of specified flags.
    #[must_use]
    pub const fn has_any_flags(&self, flags: u32) -> bool {
        (self.flags & flags) != 0
    }

    /// Returns `true` when this symbol carries type-namespace meaning only
    /// (a `TYPE_ALIAS` or `INTERFACE` with no value or alias flags).
    ///
    /// TypeScript resolves type and value namespaces independently.  When
    /// multiple wildcard re-export sources provide the same name, a pure-type
    /// declaration must not shadow a value export from a later source.  Call
    /// sites use this predicate to prefer VALUE symbols over pure-type ones.
    #[must_use]
    pub const fn is_pure_type(&self) -> bool {
        const TYPE_KINDS: u32 = symbol_flags::TYPE_ALIAS | symbol_flags::INTERFACE;
        (self.flags & TYPE_KINDS) != 0
            && (self.flags & symbol_flags::VALUE) == 0
            && (self.flags & symbol_flags::ALIAS) == 0
    }

    /// Whether a top-level declaration of this symbol is visible in the
    /// cross-file global scope.
    ///
    /// Script files (`is_external_module == false`) contribute every top-level
    /// declaration to the ambient global scope. External modules do not: a
    /// module's top-level names — including its value exports — are reachable
    /// from a *sibling* file only through an explicit `import`. An unqualified
    /// reference elsewhere is `TS2304`, and the name must never silently bind
    /// to a value export of an installed-but-unimported package (issue #12372:
    /// a bare `Symbol` must resolve to the global `SymbolConstructor`, not to a
    /// transitive package's exported `Symbol` function).
    ///
    /// The only ways an external module seeds the global scope are the ones
    /// `tsc` honors: a UMD global (`export as namespace X`) and a `declare
    /// global` augmentation. Everything else stays module-scoped.
    ///
    /// This single predicate governs both `program.globals` seeding and the
    /// `global_file_locals_index` cross-file fallback so the two tables agree.
    #[must_use]
    pub const fn is_cross_file_global(
        &self,
        is_external_module: bool,
        is_global_augmentation: bool,
    ) -> bool {
        let is_alias = self.has_any_flags(symbol_flags::ALIAS);
        (!is_alias && !is_external_module) || self.is_umd_export || is_global_augmentation
    }

    /// Record a declaration and its stable source span.
    ///
    /// Also populates the parallel [`Self::stable_declarations`] entry so
    /// that arena-less consumers (see Phase 1 of the
    /// [global query graph plan][plan]) can identify the declaration by
    /// `(file_idx, pos, end)`. At bind time the file index is left
    /// unassigned (`u32::MAX`); the driver later stamps it via
    /// [`crate::state::BinderState::stamp_file_idx`].
    ///
    /// [plan]: ../../../docs/plan/ROADMAP.md
    pub fn add_declaration(&mut self, declaration: NodeIndex, span: Option<(u32, u32)>) {
        if !self.declarations.contains(&declaration) {
            self.declarations.push(declaration);
            // Invariant: `stable_declarations` parallels `declarations`.
            // Push the stable span in lockstep so index-based iteration over
            // the two vectors stays aligned.
            self.stable_declarations
                .push(StableLocation::from_span(u32::MAX, span));
        }
    }

    /// Record the symbol's value declaration and stable source span.
    ///
    /// Also updates [`Self::stable_value_declaration`] so arena-less
    /// consumers can recover the declaration after arena eviction.
    pub const fn set_value_declaration(
        &mut self,
        declaration: NodeIndex,
        span: Option<(u32, u32)>,
    ) {
        self.value_declaration = declaration;
        self.stable_value_declaration = StableLocation::from_span(u32::MAX, span);
    }

    /// Stable source span `(pos, end)` of the first declaration, if known.
    ///
    /// Derived from the first [`StableLocation::is_known`] entry of
    /// [`Self::stable_declarations`] (declarations are pushed in binding
    /// order), falling back to [`Self::value_declaration_span`] for symbols
    /// whose only recorded declaration is a value declaration. The
    /// declaration lists are the single source of truth; no separate span
    /// field is stored.
    #[inline]
    #[must_use]
    pub fn first_declaration_span(&self) -> Option<(u32, u32)> {
        self.stable_declarations
            .iter()
            .find(|loc| loc.is_known())
            .map(|loc| (loc.pos, loc.end))
            .or_else(|| self.value_declaration_span())
    }

    /// Stable source span `(pos, end)` of the value declaration, if known.
    ///
    /// Derived from [`Self::stable_value_declaration`], which
    /// [`Self::set_value_declaration`] keeps in lockstep with
    /// [`Self::value_declaration`]. A `(0, 0)` location is the documented
    /// [`StableLocation`] "unknown" sentinel and yields `None`.
    #[inline]
    #[must_use]
    pub const fn value_declaration_span(&self) -> Option<(u32, u32)> {
        if self.stable_value_declaration.is_known() {
            Some((
                self.stable_value_declaration.pos,
                self.stable_value_declaration.end,
            ))
        } else {
            None
        }
    }

    /// Primary declaration node for this symbol: prefer `value_declaration` when
    /// set, otherwise fall back to the first entry in `declarations`. Returns
    /// `None` when neither is available.
    #[must_use]
    pub fn primary_declaration(&self) -> Option<NodeIndex> {
        self.value_declaration
            .into_option()
            .or_else(|| self.declarations.first().copied())
    }

    /// All unique declarations for this symbol, with `value_declaration` first
    /// (when set), then entries from `declarations` that are not equal to
    /// `value_declaration`. Each unique declaration appears exactly once.
    #[must_use]
    pub fn all_declarations(&self) -> Vec<NodeIndex> {
        let value_decl = self.value_declaration.into_option();
        let mut out =
            Vec::with_capacity(self.declarations.len() + usize::from(value_decl.is_some()));
        if let Some(v) = value_decl {
            out.push(v);
        }
        for d in &self.declarations {
            if Some(*d) != value_decl {
                out.push(*d);
            }
        }
        out
    }
}

// =============================================================================
// Symbol Table
// =============================================================================

/// A symbol table maps names to symbols.
/// Used for scope management and name resolution.
///
/// The inner map is `Arc`-wrapped so cloning the table is an O(1)
/// atomic refcount bump. Mutating methods (`set`, `remove`, `clear`)
/// route through `Arc::make_mut`, which is free during the typical
/// per-file binder lifecycle (refcount=1) and copy-on-writes only
/// when the table is genuinely shared. This pattern matches the
/// `SymbolArena` design above and the recently-merged `BoundFile`
/// field Arc-wraps (PRs #1399/1404/1409/1416/1428).
///
/// On declaration-heavy projects (type-fest's 263 cross-file lookup
/// binders × ~5K lib globals each) the per-file `file_locals` rebuild
/// in `create_*_binder_with_augmentations` no longer pays for a full
/// `HashMap` deep-clone of program-wide globals when callers can clone
/// a pre-built globals table; the deep-clone cost shifts to the
/// first per-file mutation, which happens at the same overall cost
/// as the prior pattern but lets cleanly-empty per-file tables stay
/// shared.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SymbolTable {
    /// Symbols indexed by their escaped name (using `FxHashMap` for faster hashing)
    symbols: Arc<FxHashMap<String, SymbolId>>,
    /// Symbols indexed by `(NodeArena identity, parsed identifier AstAtom)`.
    ///
    /// `AstAtom` values are per-arena. The arena key is part of the side-index
    /// key so tables shared across files can never resolve a same-number atom
    /// from another arena. String keys remain authoritative and are used as the
    /// fallback for cross-arena lookups, synthetic names, and deserialized
    /// tables with an empty runtime-only atom index.
    #[serde(skip)]
    atom_symbols: Arc<FxHashMap<(usize, tsz_common::interner::AstAtom), SymbolId>>,
}

impl SymbolTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            symbols: Arc::new(FxHashMap::default()),
            atom_symbols: Arc::new(FxHashMap::default()),
        }
    }

    /// Create a symbol table with pre-allocated capacity.
    /// Avoids repeated resizing when the approximate number of entries is known
    /// (e.g., class members, module exports).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            symbols: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            )),
            atom_symbols: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            )),
        }
    }

    /// Get a symbol by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<SymbolId> {
        self.symbols.get(name).copied()
    }

    /// Get a symbol by parsed identifier atom when the atom owner matches.
    #[must_use]
    pub fn get_by_atom(
        &self,
        atom_key: Option<(usize, tsz_common::interner::AstAtom)>,
    ) -> Option<SymbolId> {
        let (owner_key, atom) = atom_key?;
        if owner_key == 0 || atom == tsz_common::interner::AstAtom::NONE {
            return None;
        }
        self.atom_symbols.get(&(owner_key, atom)).copied()
    }

    /// Get a symbol by same-arena atom when present, falling back to escaped text.
    #[must_use]
    pub fn get_by_atom_or_name(
        &self,
        atom_key: Option<(usize, tsz_common::interner::AstAtom)>,
        name: &str,
    ) -> Option<SymbolId> {
        self.get_by_atom(atom_key).or_else(|| self.get(name))
    }

    /// Set a symbol by name.
    pub fn set(&mut self, name: String, symbol: SymbolId) {
        Arc::make_mut(&mut self.symbols).insert(name, symbol);
    }

    /// Set a symbol by name and by same-arena parsed identifier atom.
    pub fn set_with_atom(
        &mut self,
        name: String,
        atom_key: Option<(usize, tsz_common::interner::AstAtom)>,
        symbol: SymbolId,
    ) {
        self.set(name, symbol);
        if let Some((owner_key, atom)) = atom_key
            && owner_key != 0
            && atom != tsz_common::interner::AstAtom::NONE
        {
            Arc::make_mut(&mut self.atom_symbols).insert((owner_key, atom), symbol);
        }
    }

    /// Remove a symbol by name.
    pub fn remove(&mut self, name: &str) -> Option<SymbolId> {
        let removed = Arc::make_mut(&mut self.symbols).remove(name);
        if let Some(symbol) = removed {
            Arc::make_mut(&mut self.atom_symbols).retain(|_, id| *id != symbol);
        }
        removed
    }

    /// Check if a name exists in the table.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Get number of symbols.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Clear all symbols while keeping the allocated capacity.
    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.symbols).clear();
        Arc::make_mut(&mut self.atom_symbols).clear();
    }

    /// Iterate over symbols.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SymbolId)> {
        self.symbols.iter()
    }

    /// Merge entries from `source` whose `SymbolId` satisfies `keep`, preserving
    /// both the authoritative name keys and the same-arena `AstAtom` side-index.
    ///
    /// Used when promoting a namespace/module scope's declarations into its
    /// export table at scope exit: the source scope table already carries atom
    /// side-keys (populated by `declare_in_persistent_scope_with_atom`), and
    /// copying them forward keeps export-table lookups atom-backed instead of
    /// degrading to string-only on every namespace boundary. The name map stays
    /// authoritative, so equality and iteration over names are byte-identical to
    /// a plain string copy; the atom side-index is a pure accelerator that
    /// resolves to the same strings within the same arena.
    pub fn merge_filtered_from<F>(&mut self, source: &Self, mut keep: F)
    where
        F: FnMut(SymbolId) -> bool,
    {
        // First pass: copy the authoritative name entries for retained symbols.
        // This mirrors the prior string-only copy exactly, including which
        // names win on collision (later inserts overwrite earlier ones).
        let names = Arc::make_mut(&mut self.symbols);
        for (name, &sym_id) in source.symbols.iter() {
            if keep(sym_id) {
                names.insert(name.clone(), sym_id);
            }
        }
        // Second pass: carry the same-arena atom side-keys for retained symbols.
        // The atom map is keyed by `(arena_owner, AstAtom)` and never collides
        // across arenas, so copying retained entries cannot resolve a foreign
        // atom to a local symbol.
        if !source.atom_symbols.is_empty() {
            let atoms = Arc::make_mut(&mut self.atom_symbols);
            for (&key, &sym_id) in source.atom_symbols.iter() {
                if keep(sym_id) {
                    atoms.insert(key, sym_id);
                }
            }
        }
    }

    /// Estimate the heap bytes owned by this table (map buckets + name
    /// strings). Capacity-based estimate for residency accounting (#13249
    /// step 1); called only at perf-counter snapshot time.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // FxHashMap per-bucket overhead: hash + alignment padding.
        const BUCKET_OVERHEAD: usize = 16;
        let mut size = self.symbols.capacity()
            * (BUCKET_OVERHEAD + std::mem::size_of::<String>() + std::mem::size_of::<SymbolId>());
        size += self.atom_symbols.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<(usize, tsz_common::interner::AstAtom)>()
                + std::mem::size_of::<SymbolId>());
        for name in self.symbols.keys() {
            size += name.capacity();
        }
        size
    }
}

// =============================================================================
// Symbol Arena
// =============================================================================

/// Arena allocator for symbols.
///
/// The `name_index` field is maintained incrementally on `alloc`/`alloc_from`
/// and rebuilt automatically after deserialization. This ensures O(1) lookups
/// via `find_by_name`/`find_all_by_name` are always available without falling
/// back to a linear scan.
#[derive(Clone, Debug, Serialize, Default)]
pub struct SymbolArena {
    /// Read-only symbol prefix shared by cloned binders.
    ///
    /// User-file binders cloned from a premerged lib binder can append private
    /// symbols without first deep-copying the lib symbol universe. If a caller
    /// needs to mutate a shared-prefix symbol, [`Self::get_mut`] materializes
    /// the prefix back into `symbols`, preserving the old COW semantics.
    #[serde(default)]
    shared_prefix: Arc<Vec<Symbol>>,
    /// Arc-wrapped symbol storage for O(1) clone.
    /// During binding (refcount=1), `Arc::make_mut` is zero-cost.
    /// During checking (shared across files), no mutations occur.
    symbols: Arc<Vec<Symbol>>,
    /// Base offset for symbol IDs (0 for binder, high value for checker-local symbols)
    base_offset: u32,
    /// Name-to-SymbolId index for O(1) lookups by `escaped_name`.
    /// Maintained incrementally on `alloc`/`alloc_from`; rebuilt automatically
    /// after deserialization.
    #[serde(skip)]
    shared_name_index: Arc<FxHashMap<String, Vec<SymbolId>>>,
    /// Private append-side name-to-SymbolId index for symbols allocated after a
    /// shared prefix split.
    ///
    /// When a cloned premerged lib arena is split for append, the lib
    /// `name_index` moves into `shared_name_index` and this map starts empty.
    /// Appending a user symbol then mutates only this small private map instead
    /// of copy-on-writing the whole lib name index. If an appended symbol has
    /// the same name as a shared-prefix symbol, the shared vector for that name
    /// is copied into this map before appending so `find_all_by_name` can still
    /// return a single borrowed slice in the old lookup order.
    #[serde(skip)]
    name_index: Arc<FxHashMap<String, Vec<SymbolId>>>,
}

impl<'de> Deserialize<'de> for SymbolArena {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Helper struct that mirrors `SymbolArena` without the name index,
        /// used to leverage the derived `Deserialize` for `symbols` and `base_offset`.
        #[derive(Deserialize)]
        struct SymbolArenaRaw {
            #[serde(default)]
            shared_prefix: Arc<Vec<Symbol>>,
            symbols: Vec<Symbol>,
            base_offset: u32,
        }

        let raw = SymbolArenaRaw::deserialize(deserializer)?;
        let mut arena = Self {
            shared_prefix: raw.shared_prefix,
            symbols: Arc::new(raw.symbols),
            base_offset: raw.base_offset,
            shared_name_index: Arc::new(FxHashMap::default()),
            name_index: Arc::new(FxHashMap::default()),
        };
        arena.rebuild_name_index();
        Ok(arena)
    }
}

impl SymbolArena {
    /// Base offset for checker-local symbols to avoid ID collisions.
    pub const CHECKER_SYMBOL_BASE: u32 = 0x1000_0000;
    /// Maximum pre-allocation to avoid capacity overflow.
    const MAX_SYMBOL_PREALLOC: usize = 1_000_000;

    #[must_use]
    pub fn new() -> Self {
        Self {
            shared_prefix: Arc::new(Vec::new()),
            symbols: Arc::new(Vec::new()),
            base_offset: 0,
            shared_name_index: Arc::new(FxHashMap::default()),
            name_index: Arc::new(FxHashMap::default()),
        }
    }

    /// Create a new symbol arena with a base offset for symbol IDs.
    /// Used for checker-local symbols to avoid collisions with binder symbols.
    #[must_use]
    pub fn new_with_base(base: u32) -> Self {
        Self {
            shared_prefix: Arc::new(Vec::new()),
            symbols: Arc::new(Vec::new()),
            base_offset: base,
            shared_name_index: Arc::new(FxHashMap::default()),
            name_index: Arc::new(FxHashMap::default()),
        }
    }

    /// Create a new symbol arena with pre-allocated capacity.
    ///
    /// Pre-allocates both the symbol vector and the name index to avoid
    /// repeated reallocations during bulk insertion (e.g., the merge path).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let safe_capacity = capacity.min(Self::MAX_SYMBOL_PREALLOC);
        Self {
            shared_prefix: Arc::new(Vec::new()),
            symbols: Arc::new(Vec::with_capacity(safe_capacity)),
            base_offset: 0,
            shared_name_index: Arc::new(FxHashMap::default()),
            name_index: Arc::new(FxHashMap::with_capacity_and_hasher(
                safe_capacity,
                Default::default(),
            )),
        }
    }

    /// Allocate a new symbol and return its ID.
    ///
    /// # Panics
    ///
    /// Panics if the number of allocated symbols would overflow a `u32` when
    /// converted from arena length and added to `base_offset`.
    pub fn alloc(&mut self, flags: u32, name: String) -> SymbolId {
        let next_index = self.len();
        let id = SymbolId(
            self.base_offset
                .checked_add(u32::try_from(next_index).expect("symbol arena length exceeds u32"))
                .expect("symbol arena allocation overflows u32"),
        );
        self.push_name_index(&name, id);
        Arc::make_mut(&mut self.symbols).push(Symbol::new(id, flags, name));
        id
    }

    /// Allocate a new symbol by cloning from an existing one, with a new ID.
    /// This copies all symbol data including declarations, exports, members, etc.
    ///
    /// # Panics
    ///
    /// Panics if the number of allocated symbols would overflow a `u32` when
    /// converted from arena length and added to `base_offset`.
    pub fn alloc_from(&mut self, source: &Symbol) -> SymbolId {
        let next_index = self.len();
        let id = SymbolId(
            self.base_offset
                .checked_add(u32::try_from(next_index).expect("symbol arena length exceeds u32"))
                .expect("symbol arena allocation overflows u32"),
        );
        self.push_name_index(&source.escaped_name, id);
        let mut cloned = source.clone();
        cloned.id = id;
        Arc::make_mut(&mut self.symbols).push(cloned);
        id
    }

    /// Get a symbol by ID.
    #[inline]
    #[must_use]
    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        if id.is_none() {
            None
        } else if id.0 < self.base_offset {
            // ID is from a different arena (e.g., binder vs checker)
            None
        } else {
            let idx = (id.0 - self.base_offset) as usize;
            let shared_len = self.shared_prefix.len();
            if idx < shared_len {
                self.shared_prefix.get(idx)
            } else {
                self.symbols.get(idx - shared_len)
            }
        }
    }

    /// Get a mutable symbol by ID.
    #[inline]
    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        if id.is_none() {
            None
        } else if id.0 < self.base_offset {
            // ID is from a different arena
            None
        } else {
            let idx = (id.0 - self.base_offset) as usize;
            let shared_len = self.shared_prefix.len();
            if idx < shared_len {
                self.materialize_shared_prefix();
                Arc::make_mut(&mut self.symbols).get_mut(idx)
            } else {
                Arc::make_mut(&mut self.symbols).get_mut(idx - shared_len)
            }
        }
    }

    /// Get the number of symbols.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shared_prefix.len() + self.symbols.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shared_prefix.is_empty() && self.symbols.is_empty()
    }

    /// Iterate only symbols allocated after the shared-prefix split.
    ///
    /// User-file binders cloned from a premerged lib binder keep the lib
    /// universe in `shared_prefix` and append file-local/user symbols into
    /// `symbols`. Callers that only need to inspect user allocations can avoid
    /// walking the immutable lib prefix.
    pub fn iter_private_symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Whether the shared prefix still holds exactly the pristine premerged lib
    /// universe, i.e. all `lib_symbol_count` lib symbols reside untouched in the
    /// immutable `shared_prefix` and none were materialized back into the
    /// private `symbols` segment.
    ///
    /// A premerged-lib binder cloned for a user file moves the whole lib
    /// universe into `shared_prefix` (see
    /// [`Self::share_current_symbols_for_append`]). Any in-place mutation of a
    /// lib symbol via [`Self::get_mut`]/[`Self::iter_mut`] collapses the prefix
    /// (see `materialize_shared_prefix`), after which this returns `false`.
    ///
    /// Compaction uses this to choose between the cheap "iterate only private
    /// appended symbols" path and the full filtered scan. Centralizing the test
    /// here keeps the prefix/private layering representation owned by the arena
    /// instead of leaking a raw length comparison to `tsz-core`.
    #[must_use]
    pub fn lib_prefix_is_pristine(&self, lib_symbol_count: usize) -> bool {
        self.shared_prefix.len() == lib_symbol_count
    }

    /// Estimate the heap bytes owned by this arena: symbol slots, per-symbol
    /// heap (names, declaration lists, member tables), and the name index.
    ///
    /// Capacity-based estimate for residency accounting (#13249 step 1);
    /// walks every symbol, so call only at perf-counter snapshot time.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        const BUCKET_OVERHEAD: usize = 16;
        let mut size = self.symbols.capacity() * std::mem::size_of::<Symbol>();
        if Arc::strong_count(&self.shared_prefix) == 1 {
            size += self.shared_prefix.capacity() * std::mem::size_of::<Symbol>();
            for sym in self.shared_prefix.iter() {
                size += sym.estimated_heap_bytes();
            }
        }
        for sym in self.symbols.iter() {
            size += sym.estimated_heap_bytes();
        }
        size += self.name_index.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<String>()
                + std::mem::size_of::<Vec<SymbolId>>());
        for (name, ids) in self.name_index.iter() {
            size += name.capacity() + ids.capacity() * std::mem::size_of::<SymbolId>();
        }
        if Arc::strong_count(&self.shared_name_index) == 1 {
            size += self.shared_name_index.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<String>()
                    + std::mem::size_of::<Vec<SymbolId>>());
            for (name, ids) in self.shared_name_index.iter() {
                size += name.capacity() + ids.capacity() * std::mem::size_of::<SymbolId>();
            }
        }
        size
    }

    /// Reserve additional capacity for the symbol arena and its name index.
    /// This avoids repeated reallocations when the approximate number of
    /// upcoming symbol allocations is known.
    pub fn reserve(&mut self, additional: usize) {
        Arc::make_mut(&mut self.symbols).reserve(additional);
        Arc::make_mut(&mut self.name_index).reserve(additional);
    }

    /// Clear all symbols while keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.shared_prefix = Arc::new(Vec::new());
        self.shared_name_index = Arc::new(FxHashMap::default());
        Arc::make_mut(&mut self.symbols).clear();
        Arc::make_mut(&mut self.name_index).clear();
    }

    /// Rebuild the name index from the current symbol list.
    /// Call this after deserialization or after `reserve_symbol_ids` if
    /// indexed lookups are needed on those placeholder entries.
    pub fn rebuild_name_index(&mut self) {
        self.shared_name_index = Arc::new(FxHashMap::default());
        let name_index = Arc::make_mut(&mut self.name_index);
        name_index.clear();
        for sym in self.shared_prefix.iter() {
            if !sym.escaped_name.is_empty() {
                name_index
                    .entry(sym.escaped_name.clone())
                    .or_default()
                    .push(sym.id);
            }
        }
        for sym in self.symbols.iter() {
            if !sym.escaped_name.is_empty() {
                name_index
                    .entry(sym.escaped_name.clone())
                    .or_default()
                    .push(sym.id);
            }
        }
    }

    /// Find a symbol by name using the internal name index (O(1) lookup).
    ///
    /// This is a fallback for when scope chain lookup is not available.
    /// Note: This doesn't handle shadowing correctly - it returns the first match.
    /// For proper scoping, use the `SymbolTable` scope chain instead.
    ///
    /// The name index is always populated: incrementally via `alloc`/`alloc_from`,
    /// and automatically rebuilt after deserialization.
    #[inline]
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<SymbolId> {
        self.name_index
            .get(name)
            .or_else(|| self.shared_name_index.get(name))
            .and_then(|ids| ids.first().copied())
    }

    /// Find all symbols with a given name (O(1) lookup via name index).
    ///
    /// Returns a slice of symbol IDs that have the specified name, which can
    /// happen when declarations shadow each other or when there are conflicts.
    /// Returns an empty slice when no symbols match.
    ///
    /// The name index is always populated: incrementally via `alloc`/`alloc_from`,
    /// and automatically rebuilt after deserialization.
    #[inline]
    #[must_use]
    pub fn find_all_by_name(&self, name: &str) -> &[SymbolId] {
        self.name_index
            .get(name)
            .or_else(|| self.shared_name_index.get(name))
            .map_or(&[], Vec::as_slice)
    }

    /// Iterate over all symbols in the arena.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.shared_prefix.iter().chain(self.symbols.iter())
    }

    /// Iterate over all symbols in the arena mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Symbol> {
        self.materialize_shared_prefix();
        Arc::make_mut(&mut self.symbols).iter_mut()
    }

    /// Move the current symbol vector into a shared read-only prefix so later
    /// appends do not clone it. Symbol IDs and the name index remain valid.
    pub fn share_current_symbols_for_append(&mut self) {
        if self.symbols.is_empty() || !self.shared_prefix.is_empty() {
            return;
        }
        self.shared_prefix = Arc::clone(&self.symbols);
        self.symbols = Arc::new(Vec::new());
        self.shared_name_index = Arc::clone(&self.name_index);
        self.name_index = Arc::new(FxHashMap::default());
    }

    fn materialize_shared_prefix(&mut self) {
        if self.shared_prefix.is_empty() {
            return;
        }
        let shared_len = self.shared_prefix.len();
        let local_len = self.symbols.len();
        let mut materialized = Vec::with_capacity(shared_len + local_len);
        materialized.extend(self.shared_prefix.iter().cloned());
        materialized.extend(self.symbols.iter().cloned());
        self.shared_prefix = Arc::new(Vec::new());
        self.symbols = Arc::new(materialized);
        if !self.shared_name_index.is_empty() {
            let name_index = Arc::make_mut(&mut self.name_index);
            for (name, ids) in self.shared_name_index.iter() {
                name_index
                    .entry(name.clone())
                    .or_insert_with(|| ids.clone());
            }
            self.shared_name_index = Arc::new(FxHashMap::default());
        }
    }

    fn push_name_index(&mut self, name: &str, id: SymbolId) {
        if name.is_empty() {
            return;
        }

        let name_index = Arc::make_mut(&mut self.name_index);
        if !name_index.contains_key(name)
            && let Some(shared_ids) = self.shared_name_index.get(name)
        {
            name_index.insert(name.to_owned(), shared_ids.clone());
        }
        name_index.entry(name.to_owned()).or_default().push(id);
    }

    /// Reserve `SymbolIds` in this arena by pre-allocating placeholder symbols.
    ///
    /// This is used when copying lib `file_locals` into a user binder:
    /// - Lib has symbols 0..N (Array, String, etc.)
    /// - We copy those `SymbolIds` into user's `file_locals`
    /// - We need to reserve `SymbolIds` 0..N in user's arena so new allocations
    ///   don't overwrite lib symbols
    ///
    /// After calling this, new allocations start at N (after the reserved range).
    ///
    /// Placeholder IDs are assigned consistently with `alloc`/`alloc_from`,
    /// i.e. shifted by the arena's `base_offset`, so that `get`/`get_mut`
    /// lookups and the symbol's own `id` agree in arenas with non-zero
    /// `base_offset` (e.g. checker-local arenas).
    ///
    /// # Panics
    ///
    /// Panics if any index in `current_len..count` cannot be converted into a
    /// `u32`, or if `base_offset + index` would overflow `u32`.
    pub fn reserve_symbol_ids(&mut self, count: usize) {
        let current_len = self.len();
        if count > current_len {
            let symbols = Arc::make_mut(&mut self.symbols);
            symbols.reserve(count);
            for id in current_len..count {
                let raw_id = self
                    .base_offset
                    .checked_add(u32::try_from(id).expect("symbol ID exceeds u32"))
                    .expect("symbol ID overflows u32 with base_offset");
                symbols.push(Symbol::new(
                    SymbolId(raw_id),
                    0,
                    String::new(), // Empty placeholder
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_common::interner::AstAtom;

    fn sym() -> Symbol {
        Symbol::new(SymbolId(0), 0, String::new())
    }

    #[test]
    fn all_declarations_empty_returns_empty() {
        let s = sym();
        assert!(s.all_declarations().is_empty());
    }

    #[test]
    fn all_declarations_only_declarations() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        assert_eq!(s.all_declarations(), vec![NodeIndex(1), NodeIndex(2)]);
    }

    #[test]
    fn all_declarations_only_value_declaration() {
        let mut s = sym();
        s.set_value_declaration(NodeIndex(5), None);
        assert_eq!(s.all_declarations(), vec![NodeIndex(5)]);
    }

    #[test]
    fn all_declarations_value_first_then_others_no_duplicate() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        s.set_value_declaration(NodeIndex(2), None);
        // value_declaration should appear first, and not be duplicated.
        assert_eq!(s.all_declarations(), vec![NodeIndex(2), NodeIndex(1)]);
    }

    #[test]
    fn all_declarations_value_not_in_declarations() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        s.set_value_declaration(NodeIndex(9), None);
        assert_eq!(
            s.all_declarations(),
            vec![NodeIndex(9), NodeIndex(1), NodeIndex(2)]
        );
    }

    #[test]
    fn primary_declaration_prefers_value_declaration() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.set_value_declaration(NodeIndex(9), None);
        assert_eq!(s.primary_declaration(), Some(NodeIndex(9)));
    }

    #[test]
    fn primary_declaration_falls_back_to_first() {
        let mut s = sym();
        s.add_declaration(NodeIndex(3), None);
        s.add_declaration(NodeIndex(4), None);
        assert_eq!(s.primary_declaration(), Some(NodeIndex(3)));
    }

    #[test]
    fn primary_declaration_none_when_empty() {
        let s = sym();
        assert_eq!(s.primary_declaration(), None);
    }

    /// Pin the size of `Symbol` so accidental field growth is caught in
    /// review; every interned symbol pays this footprint. Dropping the
    /// redundant span fields (#13072 PR 1) brought this from 200 to 176 bytes;
    /// out-of-lining the import-alias payload behind `Option<Box<ImportAliasData>>`
    /// (#13072 PR 2) brought it from 176 to 136 bytes, since only the
    /// fewer-than-5% import-alias symbols now pay for the module specifier,
    /// renamed name, and resolution-mode fields.
    #[test]
    fn symbol_size_is_pinned() {
        assert_eq!(std::mem::size_of::<Symbol>(), 136);
    }

    /// The derived span accessors must reproduce the semantics of the
    /// removed stored fields: first non-`None` span across
    /// `add_declaration`/`set_value_declaration` events, and the span
    /// recorded by the last `set_value_declaration`.
    #[test]
    fn declaration_span_accessors_empty_symbol() {
        let s = sym();
        assert_eq!(s.first_declaration_span(), None);
        assert_eq!(s.value_declaration_span(), None);
    }

    #[test]
    fn declaration_span_accessors_add_then_set_same_span() {
        // The dominant binder pattern: add_declaration followed by
        // set_value_declaration with the same node and span.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), Some((10, 20)));
        s.set_value_declaration(NodeIndex(1), Some((10, 20)));
        assert_eq!(s.first_declaration_span(), Some((10, 20)));
        assert_eq!(s.value_declaration_span(), Some((10, 20)));
    }

    #[test]
    fn declaration_span_accessors_first_span_sticks_across_merges() {
        // Declaration merging: later declarations must not change the
        // first-declaration span.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), Some((10, 20)));
        s.add_declaration(NodeIndex(2), Some((30, 40)));
        s.set_value_declaration(NodeIndex(2), Some((30, 40)));
        assert_eq!(s.first_declaration_span(), Some((10, 20)));
        assert_eq!(s.value_declaration_span(), Some((30, 40)));
    }

    #[test]
    fn declaration_span_accessors_set_before_add_enum_member_pattern() {
        // Enum members call set_value_declaration before add_declaration
        // with the same node and span (binding/declaration.rs).
        let mut s = sym();
        s.set_value_declaration(NodeIndex(7), Some((5, 9)));
        s.add_declaration(NodeIndex(7), Some((5, 9)));
        assert_eq!(s.first_declaration_span(), Some((5, 9)));
        assert_eq!(s.value_declaration_span(), Some((5, 9)));
    }

    #[test]
    fn declaration_span_accessors_value_only_symbol_falls_back() {
        // A symbol whose only recorded declaration is a value declaration
        // (no add_declaration) reports the value span as its first span,
        // matching the old stored-field write in set_value_declaration.
        let mut s = sym();
        s.set_value_declaration(NodeIndex(3), Some((42, 50)));
        assert_eq!(s.first_declaration_span(), Some((42, 50)));
        assert_eq!(s.value_declaration_span(), Some((42, 50)));
    }

    #[test]
    fn declaration_span_accessors_skip_unknown_entries() {
        // A None-span declaration must not shadow a later known span,
        // matching the old "first non-None event span" semantics.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), Some((30, 40)));
        assert_eq!(s.first_declaration_span(), Some((30, 40)));
        assert_eq!(s.value_declaration_span(), None);
    }

    #[test]
    fn declaration_span_accessors_resetting_value_declaration_clears_span() {
        // The incremental prune path resets the value declaration through
        // set_value_declaration with None; the derived span must follow.
        let mut s = sym();
        s.set_value_declaration(NodeIndex(3), Some((42, 50)));
        s.set_value_declaration(NodeIndex::NONE, None);
        assert_eq!(s.value_declaration_span(), None);
    }

    #[test]
    fn symbol_table_atom_lookup_ignores_foreign_arena_atoms() {
        let lib_owner = 11;
        let user_owner = 22;
        let mut table = SymbolTable::new();

        table.set_with_atom(
            "captureEvents".to_string(),
            Some((lib_owner, AstAtom(7))),
            SymbolId(1),
        );
        table.set("globalThis".to_string(), SymbolId(2));

        assert_eq!(
            table.get_by_atom_or_name(Some((lib_owner, AstAtom(7))), "missing"),
            Some(SymbolId(1)),
            "same-arena atom lookups may use the side index"
        );
        assert_eq!(
            table.get_by_atom_or_name(Some((user_owner, AstAtom(7))), "globalThis"),
            Some(SymbolId(2)),
            "foreign atoms must not hit the same raw atom id in this table"
        );
    }

    #[test]
    fn reserve_symbol_ids_assigns_zero_based_ids_with_default_arena() {
        let mut arena = SymbolArena::new();
        arena.reserve_symbol_ids(3);
        assert_eq!(arena.len(), 3);
        for i in 0..3u32 {
            let s = arena.get(SymbolId(i)).expect("reserved symbol present");
            assert_eq!(s.id, SymbolId(i));
        }
    }

    #[test]
    fn reserve_symbol_ids_shifts_ids_by_base_offset() {
        let base = SymbolArena::CHECKER_SYMBOL_BASE;
        let mut arena = SymbolArena::new_with_base(base);
        arena.reserve_symbol_ids(4);
        assert_eq!(arena.len(), 4);

        // Each placeholder's stored id must be base_offset + index, matching
        // the contract used by `alloc`/`alloc_from` and `get`/`get_mut`.
        for i in 0..4u32 {
            let id = SymbolId(base + i);
            let s = arena
                .get(id)
                .expect("placeholder reachable via base-shifted id");
            assert_eq!(s.id, id);
        }

        // IDs below base_offset must still be rejected (different arena).
        assert!(arena.get(SymbolId(0)).is_none());
    }

    #[test]
    fn reserve_symbol_ids_then_alloc_continues_id_sequence() {
        let base = SymbolArena::CHECKER_SYMBOL_BASE;
        let mut arena = SymbolArena::new_with_base(base);
        arena.reserve_symbol_ids(2);
        let next = arena.alloc(0, String::new());
        // After reserving 2 placeholders, the next alloc must produce
        // base_offset + 2 (i.e. continue past the reserved range).
        assert_eq!(next, SymbolId(base + 2));
        assert_eq!(arena.get(next).map(|s| s.id), Some(next));
    }

    #[test]
    fn shared_prefix_name_index_survives_private_append() {
        let mut arena = SymbolArena::new();
        let array_id = arena.alloc(0, "Array".to_owned());
        arena.share_current_symbols_for_append();

        let local_id = arena.alloc(0, "Local".to_owned());

        assert_eq!(arena.find_by_name("Array"), Some(array_id));
        assert_eq!(arena.find_all_by_name("Array"), &[array_id]);
        assert_eq!(arena.find_by_name("Local"), Some(local_id));
        assert_eq!(arena.find_all_by_name("Local"), &[local_id]);
        assert!(arena.shared_name_index.contains_key("Array"));
        assert!(!arena.name_index.contains_key("Array"));
    }

    #[test]
    fn shared_prefix_name_index_preserves_duplicate_lookup_order() {
        let mut arena = SymbolArena::new();
        let shared_id = arena.alloc(0, "Iterator".to_owned());
        arena.share_current_symbols_for_append();

        let local_id = arena.alloc(0, "Iterator".to_owned());

        assert_eq!(arena.find_by_name("Iterator"), Some(shared_id));
        assert_eq!(arena.find_all_by_name("Iterator"), &[shared_id, local_id]);
        assert_eq!(
            arena.name_index.get("Iterator").map(Vec::as_slice),
            Some([shared_id, local_id].as_slice())
        );
    }

    #[test]
    fn lib_prefix_is_pristine_tracks_shared_prefix_state() {
        let mut arena = SymbolArena::new();
        arena.alloc(0, "Array".to_owned());
        arena.alloc(0, "Promise".to_owned());

        // Before sharing, nothing is in the prefix.
        assert!(!arena.lib_prefix_is_pristine(2));
        assert!(arena.lib_prefix_is_pristine(0));

        arena.share_current_symbols_for_append();
        let local = arena.alloc(0, "Local".to_owned());

        // Two lib symbols sit untouched in the shared prefix; the private
        // append (`Local`) does not affect the prefix.
        assert!(arena.lib_prefix_is_pristine(2));
        // A different reported lib count must not match the prefix.
        assert!(!arena.lib_prefix_is_pristine(3));

        // Mutating the private symbol keeps the prefix pristine.
        arena.get_mut(local).expect("local symbol").flags = 1;
        assert!(arena.lib_prefix_is_pristine(2));
    }

    #[test]
    fn lib_prefix_is_pristine_false_after_lib_symbol_materialized() {
        let mut arena = SymbolArena::new();
        let array_id = arena.alloc(0, "Array".to_owned());
        arena.alloc(0, "Promise".to_owned());
        arena.share_current_symbols_for_append();
        arena.alloc(0, "Local".to_owned());

        assert!(arena.lib_prefix_is_pristine(2));

        // Mutating a shared-prefix (lib) symbol materializes the prefix back
        // into `symbols`, collapsing the shared prefix to empty. The pristine
        // invariant must then report `false`, routing compaction to its full
        // filtered scan instead of the private-only fast path.
        arena.get_mut(array_id).expect("array symbol").flags = 1;
        assert!(!arena.lib_prefix_is_pristine(2));
    }
}
