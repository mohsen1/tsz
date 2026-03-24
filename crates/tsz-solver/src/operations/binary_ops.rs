//! Binary operation type evaluation.
//!
//! This module handles type evaluation for binary operations like:
//! - Arithmetic: +, -, *, /, %, **
//! - Comparison: ==, !=, <, >, <=, >=
//! - Logical: &&, ||, !
//! - Bitwise: &, |, ^, ~, <<, >>, >>>
//!
//! ## Architecture
//!
//! The `BinaryOpEvaluator` evaluates the result type of binary operations
//! and validates that operands are compatible with the operator.
//!
//! All functions take `TypeId` as input and return structured results,
//! making them pure logic that can be unit tested independently.

use crate::narrowing::NullishFilter;
use crate::types::TypeListId;
use crate::visitor::TypeVisitor;
use crate::{IntrinsicKind, LiteralValue, QueryDatabase, TypeData, TypeDatabase, TypeId};

/// Result of a binary operation.
#[derive(Clone, Debug, PartialEq)]
pub enum BinaryOpResult {
    /// Operation succeeded, returns the result type
    Success(TypeId),

    /// Operand type error
    TypeError {
        left: TypeId,
        right: TypeId,
        op: &'static str,
    },
}

/// Primitive type classes for overlap detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

// =============================================================================
// Visitor Pattern Implementations
// =============================================================================

/// Generate a `TypeVisitor` that checks whether a type belongs to a specific
/// primitive class (number-like, string-like, etc.).
///
/// ## Arguments
/// - `$name`: Visitor struct name
/// - `$ik`: The `IntrinsicKind` to match
/// - `$lit_pat`: Pattern to match `LiteralValue` against (use `_` for always-false)
/// - `$lit_result`: Value to return when the pattern matches
/// - Optional feature flags:
///   - `check_union_all` — `visit_union` returns true when ALL members match
///   - `check_constraint` — `visit_type_parameter/visit_infer` recurse into constraint
///   - `recurse_enum`    — `visit_enum` recurses into the member type
///   - `match_template_literal` — `visit_template_literal` returns true
///   - `check_intersection_any` — `visit_intersection` returns true when ANY member matches
macro_rules! primitive_visitor {
    ($name:ident, $ik:expr, $lit_pat:pat => $lit_result:expr $(, $feat:ident)*) => {
        struct $name<'a> { _db: &'a dyn TypeDatabase }
        impl<'a> TypeVisitor for $name<'a> {
            type Output = bool;
            fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> bool { kind == $ik }
            fn visit_literal(&mut self, value: &LiteralValue) -> bool {
                match value { $lit_pat => $lit_result, _ => false }
            }
            $(primitive_visitor!(@method $feat);)*
            fn default_output() -> bool { false }
        }
    };
    (@method check_union_all) => {
        fn visit_union(&mut self, list_id: u32) -> bool {
            let members = self._db.type_list(TypeListId(list_id));
            !members.is_empty() && members.iter().all(|&m| self.visit_type(self._db, m))
        }
    };
    (@method check_constraint) => {
        fn visit_type_parameter(&mut self, info: &crate::types::TypeParamInfo) -> bool {
            info.constraint.map(|c| self.visit_type(self._db, c)).unwrap_or(false)
        }
        fn visit_infer(&mut self, info: &crate::types::TypeParamInfo) -> bool {
            info.constraint.map(|c| self.visit_type(self._db, c)).unwrap_or(false)
        }
    };
    (@method recurse_enum) => {
        fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) -> bool {
            self.visit_type(self._db, member_type)
        }
    };
    (@method match_template_literal) => {
        fn visit_template_literal(&mut self, _template_id: u32) -> bool { true }
    };
    (@method check_intersection_any) => {
        fn visit_intersection(&mut self, list_id: u32) -> bool {
            let members = self._db.type_list(TypeListId(list_id));
            members.iter().any(|&m| self.visit_type(self._db, m))
        }
    };
}

primitive_visitor!(NumberLikeVisitor, IntrinsicKind::Number,
    LiteralValue::Number(_) => true,
    check_union_all, check_constraint, recurse_enum, check_intersection_any);

primitive_visitor!(StringLikeVisitor, IntrinsicKind::String,
    LiteralValue::String(_) => true,
    check_union_all, check_constraint, recurse_enum, match_template_literal, check_intersection_any);

primitive_visitor!(BigIntLikeVisitor, IntrinsicKind::Bigint,
    LiteralValue::BigInt(_) => true,
    check_union_all, check_constraint, recurse_enum, check_intersection_any);

primitive_visitor!(BooleanLikeVisitor, IntrinsicKind::Boolean,
    LiteralValue::Boolean(_) => true);

struct InstanceofLeftOperandVisitor<'a> {
    _db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for InstanceofLeftOperandVisitor<'a> {
    type Output = bool;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> bool {
        matches!(
            kind,
            IntrinsicKind::Any | IntrinsicKind::Unknown | IntrinsicKind::Object
        )
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> bool {
        false
    }

    fn visit_type_parameter(&mut self, _info: &crate::types::TypeParamInfo) -> bool {
        true
    }

    fn visit_object(&mut self, _shape_id: u32) -> bool {
        true
    }
    fn visit_object_with_index(&mut self, _shape_id: u32) -> bool {
        true
    }
    fn visit_array(&mut self, _element_type: TypeId) -> bool {
        true
    }
    fn visit_tuple(&mut self, _list_id: u32) -> bool {
        true
    }
    fn visit_function(&mut self, _shape_id: u32) -> bool {
        true
    }
    fn visit_callable(&mut self, _shape_id: u32) -> bool {
        true
    }
    fn visit_application(&mut self, _app_id: u32) -> bool {
        true
    }
    fn visit_lazy(&mut self, _def_id: u32) -> bool {
        // Lazy types represent interfaces/classes which are object types —
        // valid left operands for instanceof.
        true
    }
    fn visit_readonly_type(&mut self, _type_id: TypeId) -> bool {
        true
    }

    fn visit_union(&mut self, list_id: u32) -> bool {
        let members = self._db.type_list(crate::types::TypeListId(list_id));
        let mut any_valid = false;
        for &m in members.iter() {
            if self.visit_type(self._db, m) {
                any_valid = true;
                break;
            }
        }
        any_valid
    }

    fn visit_intersection(&mut self, list_id: u32) -> bool {
        let members = self._db.type_list(crate::types::TypeListId(list_id));
        let mut any_valid = false;
        for &m in members.iter() {
            if self.visit_type(self._db, m) {
                any_valid = true;
                break;
            }
        }
        any_valid
    }

    fn default_output() -> bool {
        false
    }
}

struct SymbolLikeVisitor<'a> {
    _db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for SymbolLikeVisitor<'a> {
    type Output = bool;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> bool {
        kind == IntrinsicKind::Symbol
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> bool {
        let _ = value;
        false
    }

    fn visit_ref(&mut self, _symbol_ref: u32) -> bool {
        // Named type references (interfaces, classes) are NOT symbol-like.
        // The `Symbol` wrapper object type (from lib.d.ts) is not the same
        // as the `symbol` primitive — only the primitive is valid for
        // computed property names. Unique symbols are handled by
        // visit_unique_symbol below.
        false
    }

    fn visit_unique_symbol(&mut self, _symbol_ref: u32) -> bool {
        true
    }

    fn default_output() -> bool {
        false
    }
}

/// Visitor to extract primitive class from a type.
struct PrimitiveClassVisitor;

impl TypeVisitor for PrimitiveClassVisitor {
    type Output = Option<PrimitiveClass>;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            IntrinsicKind::String => Some(PrimitiveClass::String),
            IntrinsicKind::Number => Some(PrimitiveClass::Number),
            IntrinsicKind::Boolean => Some(PrimitiveClass::Boolean),
            IntrinsicKind::Bigint => Some(PrimitiveClass::Bigint),
            IntrinsicKind::Symbol => Some(PrimitiveClass::Symbol),
            IntrinsicKind::Null => Some(PrimitiveClass::Null),
            IntrinsicKind::Undefined | IntrinsicKind::Void => Some(PrimitiveClass::Undefined),
            _ => None,
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        match value {
            LiteralValue::String(_) => Some(PrimitiveClass::String),
            LiteralValue::Number(_) => Some(PrimitiveClass::Number),
            LiteralValue::Boolean(_) => Some(PrimitiveClass::Boolean),
            LiteralValue::BigInt(_) => Some(PrimitiveClass::Bigint),
        }
    }

    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        Some(PrimitiveClass::String)
    }

    fn visit_unique_symbol(&mut self, _symbol_ref: u32) -> Self::Output {
        Some(PrimitiveClass::Symbol)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Check if an intrinsic primitive type overlaps with a literal value.
/// e.g., `string` overlaps with `"foo"`, `number` overlaps with `42`.
const fn intrinsic_overlaps_literal(kind: IntrinsicKind, value: &LiteralValue) -> bool {
    matches!(
        (kind, value),
        (IntrinsicKind::String, LiteralValue::String(_))
            | (IntrinsicKind::Number, LiteralValue::Number(_))
            | (IntrinsicKind::Boolean, LiteralValue::Boolean(_))
            | (IntrinsicKind::Bigint, LiteralValue::BigInt(_))
    )
}

/// Visitor to check type overlap for comparison operations.
struct OverlapChecker<'a> {
    db: &'a dyn TypeDatabase,
    left: TypeId,
}

impl<'a> OverlapChecker<'a> {
    fn new(db: &'a dyn TypeDatabase, left: TypeId) -> Self {
        Self { db, left }
    }

    fn check(&mut self, right: TypeId) -> bool {
        // Fast path: same type
        if self.left == right {
            return true;
        }

        // Fast path: top/bottom types
        if matches!(
            (self.left, right),
            (TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR, _)
                | (_, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
        ) {
            return true;
        }

        if self.left == TypeId::NEVER || right == TypeId::NEVER {
            return false;
        }

        // Check intersection first before visitor
        if self.db.intersection2(self.left, right) == TypeId::NEVER {
            return false;
        }

        // Use visitor to check overlap
        self.visit_type(self.db, right)
    }
}

impl<'a> TypeVisitor for OverlapChecker<'a> {
    type Output = bool;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        // Intrinsics can overlap with many things, check intersection above
        true
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        members.iter().any(|&member| self.check(member))
    }

    fn visit_type_parameter(&mut self, info: &crate::types::TypeParamInfo) -> Self::Output {
        // Unconstrained type parameters are handled in has_overlap before visitor
        match info.constraint {
            Some(constraint) => self.check(constraint),
            // Unconstrained type parameters without constraints treated as no overlap (conservative)
            None => false,
        }
    }

    fn visit_infer(&mut self, info: &crate::types::TypeParamInfo) -> Self::Output {
        // Unconstrained type parameters are handled in has_overlap before visitor
        match info.constraint {
            Some(constraint) => self.check(constraint),
            // Unconstrained type parameters without constraints treated as no overlap (conservative)
            None => false,
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        // Check if left is a literal with same value, or a supertype of the literal
        match self.db.lookup(self.left) {
            Some(TypeData::Literal(left_lit)) => left_lit == *value,
            Some(TypeData::Union(members)) => {
                // Check if left's union contains this literal or a supertype
                let members = self.db.type_list(members);
                members.iter().any(|&m| match self.db.lookup(m) {
                    Some(TypeData::Literal(lit)) => lit == *value,
                    Some(TypeData::Intrinsic(kind)) => intrinsic_overlaps_literal(kind, value),
                    _ => false,
                })
            }
            // An intrinsic primitive type overlaps with its corresponding literal type
            // e.g., `string` overlaps with `"foo"`, `number` overlaps with `42`
            Some(TypeData::Intrinsic(kind)) => intrinsic_overlaps_literal(kind, value),
            // Intersection types: if ANY member overlaps with the literal, the
            // intersection overlaps. e.g., `string & { $Brand: any }` overlaps with `""`.
            Some(TypeData::Intersection(members)) => {
                let members = self.db.type_list(members);
                members.iter().any(|&m| match self.db.lookup(m) {
                    Some(TypeData::Literal(lit)) => lit == *value,
                    Some(TypeData::Intrinsic(kind)) => intrinsic_overlaps_literal(kind, value),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    fn default_output() -> Self::Output {
        // Default: check for disjoint primitive classes
        // We conservatively return true unless we can prove they're disjoint
        // This matches the original behavior where most types are considered to overlap
        true
    }
}

/// Evaluates binary operations on types.
pub struct BinaryOpEvaluator<'a> {
    interner: &'a dyn QueryDatabase,
}

impl<'a> BinaryOpEvaluator<'a> {
    /// Create a new binary operation evaluator.
    pub fn new(interner: &'a dyn QueryDatabase) -> Self {
        Self { interner }
    }

    /// Check if a type is valid for the left side of an `instanceof` expression.
    /// TS2358: "The left-hand side of an 'instanceof' expression must be of type 'any', an object type or a type parameter."
    pub fn is_valid_instanceof_left_operand(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return true;
        }
        let mut visitor = InstanceofLeftOperandVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is valid for the right side of an `instanceof` expression.
    /// TS2359: "The right-hand side of an 'instanceof' expression must be either of type 'any', a class, function, or other type assignable to the 'Function' interface type..."
    pub fn is_valid_instanceof_right_operand<F>(
        &self,
        type_id: TypeId,
        func_ty: TypeId,
        assignable_check: &mut F,
    ) -> bool
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        if type_id == TypeId::ANY
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || type_id == TypeId::FUNCTION
        {
            return true;
        }

        if let Some(crate::TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            let mut all_valid = true;
            for &m in members.iter() {
                if !self.is_valid_instanceof_right_operand(m, func_ty, assignable_check) {
                    all_valid = false;
                    break;
                }
            }
            return all_valid && !members.is_empty();
        }

        if let Some(crate::TypeData::Intersection(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            let mut any_valid = false;
            for &m in members.iter() {
                if self.is_valid_instanceof_right_operand(m, func_ty, assignable_check) {
                    any_valid = true;
                    break;
                }
            }
            return any_valid;
        }

        // tsc: typeHasCallOrConstructSignatures — types with call/construct
        // signatures are valid instanceof RHS even if not assignable to Function.
        if self.type_has_call_or_construct_signatures(type_id) {
            return true;
        }

        assignable_check(type_id, func_ty) || assignable_check(type_id, TypeId::FUNCTION)
    }

    /// Check if a type has call or construct signatures.
    fn type_has_call_or_construct_signatures(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(crate::TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                !shape.call_signatures.is_empty() || !shape.construct_signatures.is_empty()
            }
            Some(crate::TypeData::Function(_)) => true,
            _ => false,
        }
    }

    /// Check if a type is valid for arithmetic operations (number, bigint, enum, or any).
    /// This is used for TS2362/TS2363 error checking.
    ///
    /// Also returns true for ERROR and UNKNOWN types to prevent cascading errors.
    /// If a type couldn't be resolved (TS2304, etc.), we don't want to add noise
    /// with arithmetic errors - the primary error is more useful.
    pub fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
        // Don't emit arithmetic errors for error/unknown/never types - prevents cascading errors.
        // NEVER (bottom type) is assignable to all types, so it's valid everywhere.
        if type_id == TypeId::ANY
            || type_id == TypeId::ERROR
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::NEVER
        {
            return true;
        }
        if self.is_number_like(type_id) || self.is_bigint_like(type_id) {
            return true;
        }
        // For unions like `bigint | number`, the all-number-like and all-bigint-like
        // checks both fail because neither is uniform. But each member is individually
        // a valid arithmetic type. Check if all union members are individually arithmetic.
        if let Some(members) = crate::visitor::union_list_id(self.interner, type_id) {
            let member_list = self.interner.type_list(members);
            return !member_list.is_empty()
                && member_list.iter().all(|&m| {
                    m == TypeId::ANY
                        || m == TypeId::ERROR
                        || m == TypeId::NEVER
                        || self.is_number_like(m)
                        || self.is_bigint_like(m)
                });
        }
        false
    }

    /// Evaluate a binary operation on two types.
    pub fn evaluate(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        self.evaluate_with_context(left, right, op, None)
    }

    /// Evaluate a binary operation with optional contextual type for contextual typing.
    ///
    /// This is used for logical operators where contextual target types can alter
    /// result semantics (for example, contextual function typing with `&&` can
    /// suppress false-branch unioning in assignment-compatible positions).
    pub fn evaluate_with_context(
        &self,
        left: TypeId,
        right: TypeId,
        op: &'static str,
        contextual_type: Option<TypeId>,
    ) -> BinaryOpResult {
        // `never` is the bottom type — any non-logical operation on `never` produces `never`.
        // Logical operators (&&, ||, ??) have their own `never` handling in evaluate_logical.
        if !matches!(op, "&&" | "||" | "??") && (left == TypeId::NEVER || right == TypeId::NEVER) {
            return BinaryOpResult::Success(TypeId::NEVER);
        }
        match op {
            "+" => self.evaluate_plus(left, right),
            "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
                self.evaluate_arithmetic(left, right, op)
            }
            "==" | "!=" | "===" | "!==" => {
                // Equality operators always produce boolean regardless of operand types.
                // TS2367 (no overlap) diagnostics are handled separately by the checker.
                BinaryOpResult::Success(TypeId::BOOLEAN)
            }
            "<" | ">" | "<=" | ">=" => self.evaluate_comparison(left, right, op),
            "&&" | "||" | "??" => self.evaluate_logical(left, right, op, contextual_type),
            _ => BinaryOpResult::TypeError { left, right, op },
        }
    }

    /// Evaluate a fast-path `+` chain with only type ids.
    ///
    /// Returns `Some(type)` when the chain can be resolved by checking all operands
    /// share the same primitive class (`number`, `bigint`, `string`) or contain any `any`.
    /// Returns `None` when normal binary evaluation should continue.
    pub fn evaluate_plus_chain(&self, operand_types: &[TypeId]) -> Option<TypeId> {
        if operand_types.len() <= 1 {
            return None;
        }

        let mut all_number = true;
        let mut all_bigint = true;
        let mut all_string = true;
        let mut has_any = false;

        for &operand in operand_types.iter() {
            if operand == TypeId::ERROR {
                return Some(TypeId::ERROR);
            }

            // Symbol operands need error reporting — bail to the normal path
            // so TS2469 / TS2365 can be emitted by the checker.
            if self.is_symbol_like(operand) {
                return None;
            }

            all_number &= operand == TypeId::NUMBER;
            all_bigint &= operand == TypeId::BIGINT;
            all_string &= operand == TypeId::STRING;
            has_any |= operand == TypeId::ANY;
        }

        if all_number {
            Some(TypeId::NUMBER)
        } else if all_bigint {
            Some(TypeId::BIGINT)
        } else if all_string {
            Some(TypeId::STRING)
        } else if has_any {
            Some(TypeId::ANY)
        } else {
            None
        }
    }

    /// Evaluate the + operator (can be string concatenation or addition).
    fn evaluate_plus(&self, left: TypeId, right: TypeId) -> BinaryOpResult {
        // Don't emit errors for unknown types - prevents cascading errors
        if left == TypeId::UNKNOWN || right == TypeId::UNKNOWN {
            return BinaryOpResult::Success(TypeId::UNKNOWN);
        }

        // Error types act like `any` in tsc - prevents cascading errors
        // while still inferring the correct result type (e.g., string + error = string)
        let left = if left == TypeId::ERROR {
            TypeId::ANY
        } else {
            left
        };
        let right = if right == TypeId::ERROR {
            TypeId::ANY
        } else {
            right
        };

        // TS2469: Symbol cannot be used in arithmetic
        if self.is_symbol_like(left) || self.is_symbol_like(right) {
            return BinaryOpResult::TypeError {
                left,
                right,
                op: "+",
            };
        }

        // any + anything = any (and vice versa)
        if left == TypeId::ANY || right == TypeId::ANY {
            return BinaryOpResult::Success(TypeId::ANY);
        }

        // String concatenation: string + primitive = string
        if self.is_string_like(left) || self.is_string_like(right) {
            // Check if the non-string side is a valid operand (primitive)
            let valid_left = self.is_string_like(left) || self.is_valid_string_concat_operand(left);
            let valid_right =
                self.is_string_like(right) || self.is_valid_string_concat_operand(right);

            if valid_left && valid_right {
                return BinaryOpResult::Success(TypeId::STRING);
            }
            // TS2365: Operator '+' cannot be applied to types 'string' and 'object'
            return BinaryOpResult::TypeError {
                left,
                right,
                op: "+",
            };
        }

        // number-like + number-like = number
        if self.is_number_like(left) && self.is_number_like(right) {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // bigint-like + bigint-like = bigint
        if self.is_bigint_like(left) && self.is_bigint_like(right) {
            return BinaryOpResult::Success(TypeId::BIGINT);
        }

        BinaryOpResult::TypeError {
            left,
            right,
            op: "+",
        }
    }

    /// Evaluate arithmetic operators (-, *, /, %, **).
    fn evaluate_arithmetic(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        // Don't emit errors for unknown types - prevents cascading errors
        if left == TypeId::UNKNOWN || right == TypeId::UNKNOWN {
            return BinaryOpResult::Success(TypeId::UNKNOWN);
        }

        // Error types act like `any` in tsc - prevents cascading errors
        let left = if left == TypeId::ERROR {
            TypeId::ANY
        } else {
            left
        };
        let right = if right == TypeId::ERROR {
            TypeId::ANY
        } else {
            right
        };

        // TS2469: Symbol cannot be used in arithmetic
        if self.is_symbol_like(left) || self.is_symbol_like(right) {
            return BinaryOpResult::TypeError { left, right, op };
        }

        // any allows all operations
        if left == TypeId::ANY || right == TypeId::ANY {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // number-like * number-like = number
        if self.is_number_like(left) && self.is_number_like(right) {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // bigint-like * bigint-like = bigint
        if self.is_bigint_like(left) && self.is_bigint_like(right) {
            return BinaryOpResult::Success(TypeId::BIGINT);
        }

        BinaryOpResult::TypeError { left, right, op }
    }

    /// Evaluate comparison operators (<, >, <=, >=).
    fn evaluate_comparison(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        // Don't emit errors for unknown types - prevents cascading errors
        if left == TypeId::UNKNOWN || right == TypeId::UNKNOWN {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // Error types act like `any` in tsc - prevents cascading errors
        let left = if left == TypeId::ERROR {
            TypeId::ANY
        } else {
            left
        };
        let right = if right == TypeId::ERROR {
            TypeId::ANY
        } else {
            right
        };

        // TS2469: Symbol cannot be used in comparison operators
        if self.is_symbol_like(left) || self.is_symbol_like(right) {
            return BinaryOpResult::TypeError { left, right, op };
        }

        // Any allows comparison
        if left == TypeId::ANY || right == TypeId::ANY {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // Numbers (and Enums) can be compared
        if self.is_number_like(left) && self.is_number_like(right) {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // Strings can be compared
        if self.is_string_like(left) && self.is_string_like(right) {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // BigInts can be compared
        if self.is_bigint_like(left) && self.is_bigint_like(right) {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // Booleans can be compared (valid in JS/TS)
        if self.is_boolean_like(left) && self.is_boolean_like(right) {
            return BinaryOpResult::Success(TypeId::BOOLEAN);
        }

        // Note: We intentionally do NOT have a catch-all `is_orderable` check here.
        // TSC requires both operands to be of the SAME orderable kind (both number-like,
        // both string-like, or both bigint-like). Mixed-orderable comparisons like
        // `number < string` must fall through to TypeError so the checker's
        // comparability logic (is_type_comparable_to) can handle them correctly.

        // Mismatch - emit TS2365
        BinaryOpResult::TypeError { left, right, op }
    }

    /// Evaluate logical operators (&&, ||, ??).
    fn evaluate_logical(
        &self,
        left: TypeId,
        right: TypeId,
        op: &'static str,
        contextual_type: Option<TypeId>,
    ) -> BinaryOpResult {
        let ctx = crate::narrowing::NarrowingContext::new(self.interner);

        // In contextual callable positions (e.g. `x = y && fn` where `x` is
        // function-typed), TypeScript suppresses the false-branch union when
        // the left side is boolean-like.
        if op == "&&"
            && let Some(contextual) = contextual_type
            && crate::type_queries::is_callable_type(self.interner.as_type_database(), contextual)
            && crate::type_queries::is_callable_type(self.interner.as_type_database(), right)
            && crate::relations::subtype::is_subtype_of(
                self.interner.as_type_database(),
                left,
                TypeId::BOOLEAN,
            )
        {
            let truthy_left = ctx.narrow_by_truthiness(left);
            let narrowed_result = if truthy_left == TypeId::NEVER {
                ctx.narrow_to_falsy(left)
            } else {
                right
            };
            return BinaryOpResult::Success(narrowed_result);
        }

        let result = if op == "&&" {
            // left && right
            // Use extract_definitely_falsy_type (not narrow_to_falsy) to match tsc's
            // getDefinitelyFalsyPartOfType. For `string && X`, result is `"" | X`
            // (not `string | X`), because only `""` is definitely falsy.
            let falsy_left = ctx.extract_definitely_falsy_type(left);
            let truthy_left = ctx.narrow_by_truthiness(left);

            if truthy_left == TypeId::NEVER {
                left
            } else if falsy_left == TypeId::NEVER {
                right
            } else {
                self.interner.union2(falsy_left, right)
            }
        } else if op == "||" {
            // left || right
            let truthy_left = ctx.narrow_by_truthiness(left);
            let falsy_left = ctx.narrow_to_falsy(left);

            if falsy_left == TypeId::NEVER {
                left
            } else if truthy_left == TypeId::NEVER {
                right
            } else {
                self.interner.union2(truthy_left, right)
            }
        } else {
            // left ?? right
            let non_nullish_left = ctx.narrow_by_nullishness(left, NullishFilter::ExcludeNullish);
            let nullish_left = ctx.narrow_by_nullishness(left, NullishFilter::KeepNullish);

            if nullish_left == TypeId::NEVER {
                left
            } else if non_nullish_left == TypeId::NEVER {
                right
            } else {
                self.interner.union2(non_nullish_left, right)
            }
        };

        BinaryOpResult::Success(result)
    }

    /// Check if a type is number-like (number, number literal, numeric enum, or any).
    fn is_number_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NUMBER || type_id == TypeId::ANY {
            return true;
        }
        let mut visitor = NumberLikeVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is string-like (string, string literal, template literal, or any).
    fn is_string_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || type_id == TypeId::ANY {
            return true;
        }
        let mut visitor = StringLikeVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is bigint-like (bigint, bigint literal, bigint enum, or any).
    pub fn is_bigint_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::BIGINT || type_id == TypeId::ANY {
            return true;
        }
        let mut visitor = BigIntLikeVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if two types have any overlap (can be compared).
    pub fn has_overlap(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        if left == TypeId::ANY
            || right == TypeId::ANY
            || left == TypeId::UNKNOWN
            || right == TypeId::UNKNOWN
            || left == TypeId::ERROR
            || right == TypeId::ERROR
        {
            return true;
        }
        if left == TypeId::NEVER || right == TypeId::NEVER {
            return false;
        }

        // Special handling for TypeParameter and Infer before visitor pattern
        if let Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) =
            self.interner.lookup(left)
        {
            if let Some(constraint) = info.constraint {
                return self.has_overlap(constraint, right);
            }
            return true;
        }

        if let Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) =
            self.interner.lookup(right)
        {
            if let Some(constraint) = info.constraint {
                return self.has_overlap(left, constraint);
            }
            return true;
        }

        // Handle Union types explicitly (recursively check members)
        if let Some(TypeData::Union(members)) = self.interner.lookup(left) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .any(|member| self.has_overlap(*member, right));
        }

        if let Some(TypeData::Union(members)) = self.interner.lookup(right) {
            let members = self.interner.type_list(members);
            return members.iter().any(|member| self.has_overlap(left, *member));
        }

        // Check primitive class disjointness before intersection
        if self.primitive_classes_disjoint(left, right) {
            return false;
        }

        // Check intersection before visitor pattern
        if self.interner.intersection2(left, right) == TypeId::NEVER {
            return false;
        }

        // Use visitor for remaining type checks
        let mut checker = OverlapChecker::new(self.interner, left);
        checker.check(right)
    }

    /// Check if two types belong to disjoint primitive classes.
    fn primitive_classes_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        match (self.primitive_class(left), self.primitive_class(right)) {
            (Some(left_class), Some(right_class)) => left_class != right_class,
            _ => false,
        }
    }

    /// Get the primitive class of a type (if applicable).
    fn primitive_class(&self, type_id: TypeId) -> Option<PrimitiveClass> {
        let mut visitor = PrimitiveClassVisitor;
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is symbol-like (symbol or unique symbol).
    pub fn is_symbol_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::SYMBOL {
            return true;
        }
        let mut visitor = SymbolLikeVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is a valid computed property name type (TS2464).
    ///
    /// Valid types: string, number, symbol, any (including literals, enums,
    /// template literals, unique symbols). For unions, ALL members must be valid.
    /// This check is independent of strictNullChecks.
    pub fn is_valid_computed_property_name_type(&self, type_id: TypeId) -> bool {
        self.is_valid_key_type_impl(type_id, false)
    }

    /// Check if a type is a valid mapped type constraint key type (TS2322).
    ///
    /// Like `is_valid_computed_property_name_type` but treats most deferred types
    /// as valid, since they cannot be fully resolved in generic context and will
    /// be checked at instantiation time.
    pub fn is_valid_mapped_type_key_type(&self, type_id: TypeId) -> bool {
        self.is_valid_key_type_impl(type_id, true)
    }

    fn is_valid_key_type_impl(&self, type_id: TypeId, defer_unresolved: bool) -> bool {
        if type_id == TypeId::ANY || type_id == TypeId::NEVER || type_id == TypeId::ERROR {
            return true;
        }
        match self.interner.lookup(type_id) {
            // For union types, each member must individually be valid
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|&m| self.is_valid_key_type_impl(m, defer_unresolved))
            }
            // For type parameters, check the constraint (e.g., K extends keyof T).
            // tsc uses getBaseConstraintOfType(type) for this check.
            // When the constraint is a generic Application/Lazy/Conditional that can't
            // be fully resolved (e.g., Key<U> = keyof U where U is a type param),
            // defer the check to instantiation time to avoid false TS2464.
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|c| self.is_valid_key_type_impl(c, true)),
            // keyof always produces string | number | symbol, which are all valid.
            Some(TypeData::KeyOf(_)) => true,
            // For intersection types, valid if any member is a valid key type.
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&m| self.is_valid_key_type_impl(m, defer_unresolved))
            }
            // TypeQuery (typeof expr), deferred types (generic applications, lazy refs,
            // conditionals) — try to evaluate to the underlying type. If they resolve
            // to a concrete type, check it recursively. If they remain unresolved
            // (generic context), only conservatively accept when deferring is allowed
            // (e.g., mapped type constraints). For computed property names, unresolved
            // types like interface references (e.g., Symbol) are not valid key types.
            Some(
                TypeData::TypeQuery(_)
                | TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::Conditional(_),
            ) => {
                let evaluated = self.interner.evaluate_type(type_id);
                if evaluated != type_id {
                    self.is_valid_key_type_impl(evaluated, defer_unresolved)
                } else if defer_unresolved {
                    // Unresolvable in generic context — conservatively accept
                    true
                } else {
                    // In concrete context (computed property names), unresolved types
                    // are not valid key types. E.g., Symbol interface is Lazy(DefId)
                    // that doesn't evaluate to a primitive key type.
                    false
                }
            }
            // For indexed access, try resolving first. If it remains unresolved in generic
            // context, only defer when the index constraint is compatible with the object's
            // key space; otherwise it can become `... | undefined` and is not key-like.
            Some(TypeData::IndexAccess(object_type, index_type)) if defer_unresolved => {
                let evaluated = self.interner.evaluate_type(type_id);
                if let Some(TypeData::IndexAccess(_, _)) = self.interner.lookup(evaluated) {
                    if let Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) =
                        self.interner.lookup(index_type)
                        && let Some(constraint) = info.constraint
                    {
                        return self
                            .interner
                            .is_assignable_to(constraint, self.interner.keyof(object_type));
                    }

                    true
                } else {
                    self.is_valid_key_type_impl(evaluated, defer_unresolved)
                }
            }
            _ => {
                self.is_string_like(type_id)
                    || self.is_number_like(type_id)
                    || self.is_symbol_like(type_id)
            }
        }
    }

    /// Check if a type is boolean-like (boolean or boolean literal).
    pub fn is_boolean_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::BOOLEAN || type_id == TypeId::ANY {
            return true;
        }
        let mut visitor = BooleanLikeVisitor { _db: self.interner };
        visitor.visit_type(self.interner, type_id)
    }

    /// Check if a type is a valid operand for string concatenation.
    /// Valid operands are: string, number, boolean, bigint, null, undefined, void, any.
    fn is_valid_string_concat_operand(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::ANY || type_id == TypeId::ERROR || type_id == TypeId::NEVER {
            return true;
        }
        if type_id == TypeId::UNKNOWN {
            return false;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return !members.is_empty()
                && members
                    .iter()
                    .all(|&member| self.is_valid_string_concat_operand(member));
        }
        if self.is_symbol_like(type_id) {
            return false;
        }
        // Primitives are valid
        if self.is_number_like(type_id)
            || self.is_boolean_like(type_id)
            || self.is_bigint_like(type_id)
            || type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::VOID
        {
            return true;
        }

        // Non-nullish non-symbol object/function-like types are string-concat-compatible.
        true
    }
}
