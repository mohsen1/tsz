//! Symbol types, flags, and arena for the binder.
//!
//! Provides `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`, and `symbol_flags`.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
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
// Symbol
// =============================================================================

/// Unique identifier for a symbol in the symbol table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const NONE: Self = Self(u32::MAX);

    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.0 == u32::MAX
    }

    #[must_use]
    pub const fn is_some(&self) -> bool {
        self.0 != u32::MAX
    }
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
    /// Stable source span of the first declaration, if known.
    pub first_declaration_span: Option<(u32, u32)>,
    /// First value declaration of the symbol
    pub value_declaration: NodeIndex,
    /// Stable source span of the value declaration, if known.
    pub value_declaration_span: Option<(u32, u32)>,
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
    /// Import module specifier for ES6 imports (e.g., './file' for `import { X } from './file'`)
    /// This enables resolving imported symbols to their actual exports from other files.
    pub import_module: Option<String>,
    /// Original export name for imports with renamed imports (e.g., 'foo' for `import { foo as bar }`)
    /// If None, the import name matches the `escaped_name`.
    pub import_name: Option<String>,
    /// Whether this symbol is a UMD namespace export (`export as namespace Foo`).
    /// UMD exports are ALIAS symbols that should be globally visible across files,
    /// unlike regular import aliases which are file-local.
    pub is_umd_export: bool,
}

impl Symbol {
    /// Create a new symbol with the given flags and name.
    #[must_use]
    pub const fn new(id: SymbolId, flags: u32, name: String) -> Self {
        Self {
            flags,
            escaped_name: name,
            declarations: Vec::new(),
            first_declaration_span: None,
            value_declaration: NodeIndex::NONE,
            value_declaration_span: None,
            parent: SymbolId::NONE,
            id,
            exports: None,
            members: None,
            is_exported: false,
            is_type_only: false,
            decl_file_idx: u32::MAX,
            import_module: None,
            import_name: None,
            is_umd_export: false,
        }
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

    /// Record a declaration and its stable source span.
    pub fn add_declaration(&mut self, declaration: NodeIndex, span: Option<(u32, u32)>) {
        if !self.declarations.contains(&declaration) {
            self.declarations.push(declaration);
        }
        if self.first_declaration_span.is_none() {
            self.first_declaration_span = span;
        }
    }

    /// Record the symbol's value declaration and stable source span.
    pub fn set_value_declaration(&mut self, declaration: NodeIndex, span: Option<(u32, u32)>) {
        self.value_declaration = declaration;
        self.value_declaration_span = span;
        if self.first_declaration_span.is_none() {
            self.first_declaration_span = span;
        }
    }
}

// =============================================================================
// Symbol Table
// =============================================================================

/// A symbol table maps names to symbols.
/// Used for scope management and name resolution.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SymbolTable {
    /// Symbols indexed by their escaped name (using `FxHashMap` for faster hashing)
    symbols: FxHashMap<String, SymbolId>,
}

impl SymbolTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            symbols: FxHashMap::default(),
        }
    }

    /// Get a symbol by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<SymbolId> {
        self.symbols.get(name).copied()
    }

    /// Set a symbol by name.
    pub fn set(&mut self, name: String, symbol: SymbolId) {
        self.symbols.insert(name, symbol);
    }

    /// Remove a symbol by name.
    pub fn remove(&mut self, name: &str) -> Option<SymbolId> {
        self.symbols.remove(name)
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
        self.symbols.clear();
    }

    /// Iterate over symbols.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SymbolId)> {
        self.symbols.iter()
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
    symbols: Vec<Symbol>,
    /// Base offset for symbol IDs (0 for binder, high value for checker-local symbols)
    base_offset: u32,
    /// Name-to-SymbolId index for O(1) lookups by `escaped_name`.
    /// Maintained incrementally on `alloc`/`alloc_from`; rebuilt automatically
    /// after deserialization.
    #[serde(skip)]
    name_index: FxHashMap<String, Vec<SymbolId>>,
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
            symbols: Vec<Symbol>,
            base_offset: u32,
        }

        let raw = SymbolArenaRaw::deserialize(deserializer)?;
        let mut arena = Self {
            symbols: raw.symbols,
            base_offset: raw.base_offset,
            name_index: FxHashMap::default(),
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
            symbols: Vec::new(),
            base_offset: 0,
            name_index: FxHashMap::default(),
        }
    }

    /// Create a new symbol arena with a base offset for symbol IDs.
    /// Used for checker-local symbols to avoid collisions with binder symbols.
    #[must_use]
    pub fn new_with_base(base: u32) -> Self {
        Self {
            symbols: Vec::new(),
            base_offset: base,
            name_index: FxHashMap::default(),
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
            symbols: Vec::with_capacity(safe_capacity),
            base_offset: 0,
            name_index: FxHashMap::with_capacity_and_hasher(safe_capacity, Default::default()),
        }
    }

    /// Allocate a new symbol and return its ID.
    ///
    /// # Panics
    ///
    /// Panics if the number of allocated symbols would overflow a `u32` when
    /// converted from arena length and added to `base_offset`.
    pub fn alloc(&mut self, flags: u32, name: String) -> SymbolId {
        let id = SymbolId(
            self.base_offset
                + u32::try_from(self.symbols.len()).expect("symbol arena length exceeds u32"),
        );
        if !name.is_empty() {
            self.name_index.entry(name.clone()).or_default().push(id);
        }
        self.symbols.push(Symbol::new(id, flags, name));
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
        let id = SymbolId(
            self.base_offset
                + u32::try_from(self.symbols.len()).expect("symbol arena length exceeds u32"),
        );
        if !source.escaped_name.is_empty() {
            self.name_index
                .entry(source.escaped_name.clone())
                .or_default()
                .push(id);
        }
        let mut cloned = source.clone();
        cloned.id = id;
        self.symbols.push(cloned);
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
            self.symbols.get((id.0 - self.base_offset) as usize)
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
            self.symbols.get_mut((id.0 - self.base_offset) as usize)
        }
    }

    /// Get the number of symbols.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Clear all symbols while keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.name_index.clear();
    }

    /// Rebuild the name index from the current symbol list.
    /// Call this after deserialization or after `reserve_symbol_ids` if
    /// indexed lookups are needed on those placeholder entries.
    pub fn rebuild_name_index(&mut self) {
        self.name_index.clear();
        for sym in &self.symbols {
            if !sym.escaped_name.is_empty() {
                self.name_index
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
        self.name_index.get(name).map_or(&[], Vec::as_slice)
    }

    /// Iterate over all symbols in the arena.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Iterate over all symbols in the arena mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Symbol> {
        self.symbols.iter_mut()
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
    /// # Panics
    ///
    /// Panics if any index in `current_len..count` cannot be converted into a
    /// `u32`.
    pub fn reserve_symbol_ids(&mut self, count: usize) {
        let current_len = self.symbols.len();
        if count > current_len {
            // Extend with placeholder symbols to reserve the SymbolIds
            self.symbols.reserve(count);
            for id in current_len..count {
                self.symbols.push(Symbol::new(
                    SymbolId(u32::try_from(id).expect("symbol ID exceeds u32")),
                    0,
                    String::new(), // Empty placeholder
                ));
            }
        }
    }
}
