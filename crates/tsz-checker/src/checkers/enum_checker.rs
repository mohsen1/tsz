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
            return symbol.has_any_flags(symbol_flags::ENUM);
        }
        false
    }

    /// Check if a type is a *numeric* enum type or union of *numeric* enum member
    /// literal types, for the purpose of arithmetic operand validation.
    ///
    /// This handles cases like `type YesNo = Choice.Yes | Choice.No` where the
    /// type is a union of `Lazy(DefId)` references to enum members. The solver's
    /// `NumberLikeVisitor` can't resolve these, but the checker can via symbol
    /// resolution.
    ///
    /// **Important:** This should only be used as a fallback when the resolved type
    /// is still `Lazy(DefId)` (i.e., `evaluate_type_with_env` couldn't fully resolve
    /// the type). When the type IS resolved, `BinaryOpEvaluator::is_arithmetic_operand`
    /// already handles `Enum(DefId, member_type)` correctly via the visitor pattern,
    /// distinguishing numeric from string enums by checking the member type.
    ///
    /// Returns true if:
    /// - The type is a direct enum type (via `is_enum_type`)
    /// - The type is a union where every member resolves to an ENUM or `ENUM_MEMBER` symbol
    pub fn is_enum_like_type(&self, type_id: TypeId) -> bool {
        // Fast path: direct enum type
        if self.is_enum_type(type_id) {
            return true;
        }

        // Check if it's a union where all members are enum/enum-member types
        let Some(members) = crate::query_boundaries::common::union_list_id(self.ctx.types, type_id)
        else {
            // Not a union — check if it's a single enum member
            return self.is_enum_or_enum_member(type_id);
        };

        let member_list = self.ctx.types.type_list(members);
        if member_list.is_empty() {
            return false;
        }

        member_list.iter().all(|&m| self.is_enum_or_enum_member(m))
    }

    /// Check if a resolved type is still an unresolved `Lazy(DefId)`.
    ///
    /// Used to determine whether the `is_enum_like_type` fallback should apply.
    /// When `evaluate_type_with_env` resolves the type (to `Enum`, `Literal`, etc.),
    /// `is_arithmetic_operand` is authoritative and no fallback is needed.
    /// Only when the type stays as `Lazy` (evaluation couldn't resolve it) should
    /// the symbol-based enum check be used as a fallback.
    pub fn is_unresolved_lazy_type(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::is_lazy_type(self.ctx.types, type_id)
    }

    /// Check if a type resolves to an ENUM or `ENUM_MEMBER` symbol.
    fn is_enum_or_enum_member(&self, type_id: TypeId) -> bool {
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            return symbol.has_any_flags(symbol_flags::ENUM | symbol_flags::ENUM_MEMBER);
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

        if !symbol.has_any_flags(symbol_flags::INTERFACE) {
            return false;
        }

        matches!(
            symbol.escaped_name.as_str(),
            "Number" | "String" | "Boolean" | "BigInt" | "Symbol"
        )
    }
}
