//! Boxed-primitive type detection utilities.
//!
//! This module extends `CheckerState` with boxed-primitive detection for
//! TS2362/TS2363/TS2365. Enum semantic predicates live behind
//! `query_boundaries::enum_analysis`.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

        if !symbol.has_any_flags(symbol_flags::INTERFACE) {
            return false;
        }

        matches!(
            symbol.escaped_name.as_str(),
            "Number" | "String" | "Boolean" | "BigInt" | "Symbol"
        )
    }
}
