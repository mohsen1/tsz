//! Enum and boxed-primitive type detection utilities.
//!
//! This module extends `CheckerState` with enum detection (for binary
//! operator type checking) and boxed-primitive detection (for TS2362/TS2363/TS2365).

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type is an enum type.
    ///
    /// Returns true if the type represents a TypeScript enum.
    pub fn is_enum_type(&self, type_id: TypeId) -> bool {
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return (symbol.flags & symbol_flags::ENUM) != 0;
        }
        false
    }

    /// Check if a type is a boxed primitive type (Number, String, Boolean, `BigInt`, Symbol).
    ///
    /// TypeScript has two representations for primitives:
    /// - `number`, `string`, `boolean` - primitive types (valid for arithmetic)
    /// - `Number`, `String`, `Boolean` - interface wrapper types from lib.d.ts (NOT valid for arithmetic)
    ///
    /// This method detects the boxed interface types to emit proper TS2362/TS2363/TS2365 errors.
    pub fn is_boxed_primitive_type(&self, type_id: TypeId) -> bool {
        let sym_id = match self.ctx.resolve_type_to_symbol_id(type_id) {
            Some(sym_id) => sym_id,
            None => return false,
        };

        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(symbol) => symbol,
            None => return false,
        };

        if (symbol.flags & symbol_flags::INTERFACE) == 0 {
            return false;
        }

        matches!(
            symbol.escaped_name.as_str(),
            "Number" | "String" | "Boolean" | "BigInt" | "Symbol"
        )
    }
}
