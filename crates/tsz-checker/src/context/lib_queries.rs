//! Library and global type availability queries for `CheckerContext`.
//!
//! These methods check whether specific types (Promise, Symbol, etc.) are
//! available in lib files or global scope, and classify known global types
//! for diagnostic selection (TS2304 vs TS2318 vs TS2583).

use std::sync::Arc;

use tsz_binder::SymbolId;

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Check if the Promise constructor VALUE is available.
    /// The ES5 lib declares `interface Promise<T>` (type only) but NOT
    /// `declare var Promise: PromiseConstructor` (value). ES2015+ libs declare both.
    /// Used for TS2705: "An async function in ES5 requires the Promise constructor."
    pub fn has_promise_constructor_in_scope(&self) -> bool {
        use tsz_binder::symbol_flags;
        // Fast-path: if PromiseConstructor type is present in loaded libs/scope,
        // treat Promise constructor as available even if VALUE flags were not merged.
        if self.has_name_in_lib("PromiseConstructor") {
            return true;
        }
        // Check if Promise exists as a VALUE symbol (not just a TYPE)
        let check_symbol_has_value =
            |sym_id: tsz_binder::SymbolId, binder: &tsz_binder::BinderState| -> bool {
                if let Some(sym) = binder.symbols.get(sym_id) {
                    (sym.flags & symbol_flags::VALUE) != 0
                } else {
                    false
                }
            };

        // Check lib contexts
        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get("Promise")
                && check_symbol_has_value(sym_id, &lib_ctx.binder)
            {
                return true;
            }
        }

        // Check current scope
        if let Some(sym_id) = self.binder.current_scope.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        // Check file locals
        if let Some(sym_id) = self.binder.file_locals.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        false
    }

    /// Check whether Promise-constructor-based features should report missing-runtime diagnostics.
    ///
    /// This is intentionally based on the loaded libs / declarations, not on the
    /// `target` alone. Conformance cases like `@target: es2015` with `@lib: es5`
    /// still need TS2468/TS2705/TS2712 because the Promise value is absent.
    pub fn promise_constructor_diagnostics_required(&self) -> bool {
        !self.has_promise_constructor_in_scope()
    }

    /// Check if Symbol is available in lib files or global scope.
    /// Returns true if Symbol is declared in lib contexts, globals, or type declarations.
    pub fn has_symbol_in_lib(&self) -> bool {
        // Check lib contexts first
        for lib_ctx in self.lib_contexts.iter() {
            if lib_ctx.binder.file_locals.has("Symbol") {
                return true;
            }
        }

        // Check if Symbol is available in current scope/global context
        if self.binder.current_scope.has("Symbol") {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has("Symbol") {
            return true;
        }

        false
    }

    /// Check if a named symbol is available in lib files or global scope.
    /// Returns true if the symbol is declared in lib contexts, globals, or current scope.
    /// This is a generalized version of `has_symbol_in_lib` for any symbol name.
    pub fn has_name_in_lib(&self, name: &str) -> bool {
        // Check lib contexts first
        for lib_ctx in self.lib_contexts.iter() {
            if lib_ctx.binder.file_locals.has(name) {
                return true;
            }
        }

        // Check if symbol is available in current scope/global context
        if self.binder.current_scope.has(name) {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has(name) {
            return true;
        }

        false
    }

    /// Check if a symbol originates from a lib context.
    pub fn symbol_is_from_lib(&self, sym_id: SymbolId) -> bool {
        let Some(symbol_arena) = self.binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        self.lib_contexts
            .iter()
            .any(|lib_ctx| Arc::ptr_eq(&lib_ctx.arena, symbol_arena))
    }

    /// Check if a name is a known global type that should emit TS2318/TS2583 when missing.
    /// This helps distinguish between "unknown name" (TS2304) and "missing global type" (TS2318/TS2583).
    pub fn is_known_global_type(&self, name: &str) -> bool {
        use tsz_binder::lib_loader;

        // ES2015+ types
        if lib_loader::is_es2015_plus_type(name) {
            return true;
        }

        // Pre-ES2015 global types that are commonly used
        // These are always available in lib.d.ts but should emit TS2318 when @noLib is enabled
        matches!(
            name,
            "Object"
                | "Function"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Date"
                | "RegExp"
                | "Error"
                | "Math"
                | "JSON"
                | "console"
                | "window"
                | "document"
                | "ArrayBuffer"
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
                | "this"
                | "globalThis"
                | "IArguments"
        )
    }
}
