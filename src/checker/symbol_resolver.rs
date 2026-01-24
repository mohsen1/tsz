//! Symbol Resolver Module
//!
//! This module contains symbol resolution methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Symbol type resolution helpers
//! - Global intrinsic detection
//! - Symbol information queries
//!
//! This module extends CheckerState with additional methods for symbol-related
//! operations, providing cleaner APIs for common patterns.

use crate::binder::SymbolId;
use crate::checker::state::CheckerState;
use crate::solver::TypeId;

// =============================================================================
// Symbol Resolution Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Symbol Type Resolution
    // =========================================================================

    /// Get the type of a symbol with caching.
    ///
    /// This is a convenience wrapper around `get_type_of_symbol` that
    /// provides a clearer name for the operation.
    pub fn get_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        self.get_type_of_symbol(sym_id)
    }

    // =========================================================================
    // Global Symbol Detection
    // =========================================================================

    /// Check if a name refers to a global intrinsic value.
    ///
    /// Returns true for names like `undefined`, `NaN`, `Infinity`, etc.
    pub fn is_global_intrinsic(&self, name: &str) -> bool {
        matches!(
            name,
            "undefined"
                | "NaN"
                | "Infinity"
                | "Math"
                | "JSON"
                | "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Date"
                | "RegExp"
                | "Error"
                | "Function"
                | "Promise"
        )
    }

    /// Check if a name refers to a global constructor.
    ///
    /// Returns true for built-in constructor names like `Object`, `Array`, etc.
    pub fn is_global_constructor(&self, name: &str) -> bool {
        matches!(
            name,
            "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Date"
                | "RegExp"
                | "Error"
                | "Function"
                | "Promise"
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "Proxy"
                | "Reflect"
        )
    }

    // =========================================================================
    // Symbol Information Queries
    // =========================================================================

    /// Get the name of a symbol.
    ///
    /// Returns the symbol's name as a string, or None if the symbol doesn't exist.
    pub fn get_symbol_name(&self, sym_id: SymbolId) -> Option<String> {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.escaped_name.clone())
    }

    /// Check if a symbol is exported.
    ///
    /// Returns true if the symbol has the exported flag set.
    pub fn is_symbol_exported(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.is_exported)
            .unwrap_or(false)
    }

    /// Check if a symbol is type-only (e.g., from `import type`).
    ///
    /// Returns true if the symbol has the type-only flag set.
    pub fn is_symbol_type_only(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.is_type_only)
            .unwrap_or(false)
    }

    // =========================================================================
    // Symbol Property Queries
    // =========================================================================

    /// Get the value declaration of a symbol.
    ///
    /// Returns the primary value declaration node for the symbol, if any.
    pub fn get_symbol_value_declaration(&self, sym_id: SymbolId) -> Option<crate::parser::NodeIndex> {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .and_then(|symbol| {
                let decl = symbol.value_declaration;
                if decl.0 != u32::MAX {
                    Some(decl)
                } else {
                    None
                }
            })
    }

    /// Get all declarations for a symbol.
    ///
    /// Returns all declaration nodes associated with the symbol.
    pub fn get_symbol_declarations(&self, sym_id: SymbolId) -> Vec<crate::parser::NodeIndex> {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.declarations.clone())
            .unwrap_or_default()
    }

    /// Check if a symbol has a specific flag.
    ///
    /// Returns true if the symbol has the specified flag bit set.
    pub fn symbol_has_flag(&self, sym_id: SymbolId, flag: u32) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| (symbol.flags & flag) != 0)
            .unwrap_or(false)
    }

    /// Check if a symbol is a class symbol.
    ///
    /// Returns true if any of the symbol's declarations is a class declaration.
    pub fn is_class_symbol(&self, sym_id: SymbolId) -> bool {
        self.get_symbol_declarations(sym_id)
            .iter()
            .any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_class(node))
                    .is_some()
            })
    }

    /// Check if a symbol is an interface symbol.
    ///
    /// Returns true if any of the symbol's declarations is an interface declaration.
    pub fn is_interface_symbol(&self, sym_id: SymbolId) -> bool {
        self.get_symbol_declarations(sym_id)
            .iter()
            .any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_interface(node))
                    .is_some()
            })
    }

    /// Check if a symbol is an enum symbol.
    ///
    /// Returns true if any of the symbol's declarations is an enum declaration.
    pub fn is_enum_symbol(&self, sym_id: SymbolId) -> bool {
        self.get_symbol_declarations(sym_id)
            .iter()
            .any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_enum_declaration(node))
                    .is_some()
            })
    }

    /// Check if a symbol is a type alias symbol.
    ///
    /// Returns true if any of the symbol's declarations is a type alias declaration.
    pub fn is_type_alias_symbol(&self, sym_id: SymbolId) -> bool {
        self.get_symbol_declarations(sym_id)
            .iter()
            .any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_type_alias(node))
                    .is_some()
            })
    }

    /// Get the flags for a symbol.
    ///
    /// Returns the symbol's flag bits, or 0 if the symbol doesn't exist.
    pub fn get_symbol_flags(&self, sym_id: SymbolId) -> u32 {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.flags)
            .unwrap_or(0)
    }

    /// Check if a symbol is declared as const.
    ///
    /// Returns true if the symbol has the const flag set.
    pub fn is_const_symbol(&self, sym_id: SymbolId) -> bool {
        self.symbol_has_flag(sym_id, crate::binder::symbol_flags::CONST)
    }

    /// Check if a symbol is declared as let.
    ///
    /// Returns true if the symbol has the let flag set.
    pub fn is_let_symbol(&self, sym_id: SymbolId) -> bool {
        self.symbol_has_flag(sym_id, crate::binder::symbol_flags::LET)
    }
}
