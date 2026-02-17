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

use crate::types::TypeListId;
use crate::visitor::TypeVisitor;
use crate::{IntrinsicKind, LiteralValue, TypeData, TypeDatabase, TypeId};

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
pub enum PrimitiveClass {
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
///   - `ref_conservative`— `visit_ref` returns true (conservative for unresolved enums)
///   - `match_template_literal` — `visit_template_literal` returns true
///   - `match_unique_symbol`    — `visit_unique_symbol` returns true
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
    (@method ref_conservative) => {
        fn visit_ref(&mut self, _symbol_ref: u32) -> bool { true }
    };
    (@method match_template_literal) => {
        fn visit_template_literal(&mut self, _template_id: u32) -> bool { true }
    };
    (@method match_unique_symbol) => {
        fn visit_unique_symbol(&mut self, _symbol_ref: u32) -> bool { true }
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
    check_constraint, recurse_enum, match_template_literal, check_intersection_any);

primitive_visitor!(BigIntLikeVisitor, IntrinsicKind::Bigint,
    LiteralValue::BigInt(_) => true,
    check_union_all, check_constraint, recurse_enum, check_intersection_any);

primitive_visitor!(BooleanLikeVisitor, IntrinsicKind::Boolean,
    LiteralValue::Boolean(_) => true);

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
        true
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
            None => panic!("TypeParameter without constraint should not reach visitor"),
        }
    }

    fn visit_infer(&mut self, info: &crate::types::TypeParamInfo) -> Self::Output {
        // Unconstrained type parameters are handled in has_overlap before visitor
        match info.constraint {
            Some(constraint) => self.check(constraint),
            None => panic!("Infer without constraint should not reach visitor"),
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
    interner: &'a dyn TypeDatabase,
}

impl<'a> BinaryOpEvaluator<'a> {
    /// Create a new binary operation evaluator.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        Self { interner }
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
        self.is_number_like(type_id) || self.is_bigint_like(type_id)
    }

    /// Evaluate a binary operation on two types.
    pub fn evaluate(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        match op {
            "+" => self.evaluate_plus(left, right),
            "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
                self.evaluate_arithmetic(left, right, op)
            }
            "==" | "!=" | "===" | "!==" => {
                if self.has_overlap(left, right) {
                    BinaryOpResult::Success(TypeId::BOOLEAN)
                } else {
                    BinaryOpResult::TypeError { left, right, op }
                }
            }
            "<" | ">" | "<=" | ">=" => self.evaluate_comparison(left, right),
            "&&" | "||" => self.evaluate_logical(left, right),
            _ => BinaryOpResult::TypeError { left, right, op },
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
    fn evaluate_comparison(&self, left: TypeId, right: TypeId) -> BinaryOpResult {
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
            return BinaryOpResult::TypeError {
                left,
                right,
                op: "<",
            };
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

        // Mismatch - emit TS2365
        BinaryOpResult::TypeError {
            left,
            right,
            op: "<",
        }
    }

    /// Evaluate logical operators (&&, ||).
    fn evaluate_logical(&self, left: TypeId, right: TypeId) -> BinaryOpResult {
        // For && and ||, TypeScript returns a union of the two types
        BinaryOpResult::Success(self.interner.union2(left, right))
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
    fn is_bigint_like(&self, type_id: TypeId) -> bool {
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
        if type_id == TypeId::ANY || type_id == TypeId::NEVER || type_id == TypeId::ERROR {
            return true;
        }
        // For union types, each member must individually be valid
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return !members.is_empty()
                && members
                    .iter()
                    .all(|&m| self.is_valid_computed_property_name_type(m));
        }
        self.is_string_like(type_id) || self.is_number_like(type_id) || self.is_symbol_like(type_id)
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
