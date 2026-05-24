//! Shared binary comparison and display helpers.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the operator name for a unary operator token (for TS17006 error messages).
    ///
    /// Returns the string representation of unary operators that are not allowed
    /// on the left-hand side of exponentiation (`**`).
    pub(super) const fn unary_operator_name(op: u16) -> Option<&'static str> {
        match op {
            k if k == SyntaxKind::MinusToken as u16 => Some("-"),
            k if k == SyntaxKind::PlusToken as u16 => Some("+"),
            k if k == SyntaxKind::TildeToken as u16 => Some("~"),
            k if k == SyntaxKind::ExclamationToken as u16 => Some("!"),
            k if k == SyntaxKind::TypeOfKeyword as u16 => Some("typeof"),
            k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
            k if k == SyntaxKind::DeleteKeyword as u16 => Some("delete"),
            _ => None,
        }
    }

    /// Find the callable truthiness body for a logical operator expression.
    ///
    /// When a logical expression (`&&`, `||`, `??`) is part of an `if` condition,
    /// this returns the then-branch statement for callable truthiness checking.
    /// It walks up through nested logical expressions and parentheses to find
    /// the containing `if` statement.
    pub(super) fn find_callable_truthiness_body(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut parent_idx = self.ctx.arena.get_extended(idx)?.parent;
        if parent_idx.is_none() {
            return None;
        }

        loop {
            let parent = self.ctx.arena.get(parent_idx)?;
            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || matches!(
                    self.ctx.arena.get_binary_expr(parent),
                    Some(bin)
                        if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                            || bin.operator_token == SyntaxKind::BarBarToken as u16
                            || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                )
            {
                parent_idx = self.ctx.arena.get_extended(parent_idx)?.parent;
                continue;
            }

            break if parent.kind == syntax_kind_ext::IF_STATEMENT {
                self.ctx
                    .arena
                    .get_if_statement(parent)
                    .map(|if_stmt| if_stmt.then_statement)
            } else {
                None
            };
        }
    }

    /// If `idx` is a `typeof` expression (`PREFIX_UNARY_EXPRESSION` with `TypeOfKeyword`),
    /// return the typeof result type:
    /// `"string" | "number" | "bigint" | "boolean" | "symbol" | "undefined" | "object" | "function"`.
    /// This is used for TS2367 overlap detection so that comparisons like
    /// `typeof x == "Object"` (capital O) correctly detect no overlap.
    pub(super) fn typeof_result_type_if_typeof(&self, idx: NodeIndex) -> Option<TypeId> {
        use tsz_scanner::SyntaxKind;
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.ctx.arena.get_unary_expr(node)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }
        let factory = self.ctx.types.factory();
        let members = vec![
            factory.literal_string("string"),
            factory.literal_string("number"),
            factory.literal_string("bigint"),
            factory.literal_string("boolean"),
            factory.literal_string("symbol"),
            factory.literal_string("undefined"),
            factory.literal_string("object"),
            factory.literal_string("function"),
        ];
        Some(factory.union(members))
    }

    /// Check if an identifier node's declared type overlaps with the given comparison type.
    /// Returns true if the identifier's declared type is wider than `narrow_type` and
    /// has overlap with `other_type`. This prevents false TS2367 when flow narrowing
    /// inside loops makes the narrowed type too specific (e.g., `0` instead of `0 | 1`).
    pub(super) fn declared_type_has_overlap_in_loop(
        &mut self,
        comparison_idx: NodeIndex,
        idx: NodeIndex,
        narrow_type: TypeId,
        other_type: TypeId,
    ) -> bool {
        if !self.is_inside_loop(comparison_idx) {
            return false;
        }

        let node = match self.ctx.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        // Only applies to identifiers
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        // Resolve the identifier to a symbol
        let sym_id = match self.ctx.binder.resolve_identifier(self.ctx.arena, idx) {
            Some(s) => s,
            None => return false,
        };
        // Get the symbol's value_declaration and its type (the declared type)
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(s) => s,
            None => return false,
        };
        if symbol.value_declaration.is_none() {
            return false;
        }
        let declared_type = match self.ctx.node_types.get(&symbol.value_declaration.0) {
            Some(&t) => t,
            None => return false,
        };
        // Only relevant when the declared type is wider than the narrowed type
        if declared_type == narrow_type {
            return false;
        }
        // Check if the declared type overlaps with the other operand
        !self.types_have_no_overlap(declared_type, other_type)
    }

    fn is_inside_loop(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::WHILE_STATEMENT
                    || k == syntax_kind_ext::DO_STATEMENT
                    || k == syntax_kind_ext::FOR_STATEMENT
                    || k == syntax_kind_ext::FOR_IN_STATEMENT
                    || k == syntax_kind_ext::FOR_OF_STATEMENT
            ) {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Get the primitive type family of a type: `TypeId::STRING` for string/string literals,
    /// `TypeId::NUMBER` for number/number literals, `TypeId::BOOLEAN` for boolean/boolean literals,
    /// `TypeId::BIGINT` for bigint/bigint literals, or `TypeId::ERROR` for non-primitive types.
    ///
    /// Used to determine if two types are from different primitive families (e.g., string vs number)
    /// for TS2367 display purposes. When types are from different families, tsc widens literals
    /// to their base primitive types in error messages.
    fn get_primitive_family(&self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::LiteralTypeKind;
        use crate::query_boundaries::common::{
            classify_literal_type, is_string_intrinsic_type, is_template_literal_type,
            is_unique_symbol_type,
        };

        // Check direct primitive type IDs
        if type_id == TypeId::STRING
            || type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::BIGINT
            || type_id == TypeId::SYMBOL
        {
            return type_id;
        }

        // Boolean literal intrinsics (`true` / `false`) belong to the boolean
        // family. classify_literal_type below short-circuits on intrinsics,
        // so we'd otherwise miss them and TS2367 cross-family widening
        // would skip — leaving messages like `'symbol' and 'true'` instead
        // of tsc's `'symbol' and 'boolean'`.
        if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
            return TypeId::BOOLEAN;
        }

        // Check literal types via query boundary
        match classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(_) => return TypeId::STRING,
            LiteralTypeKind::Number(_) => return TypeId::NUMBER,
            LiteralTypeKind::Boolean(_) => return TypeId::BOOLEAN,
            LiteralTypeKind::BigInt(_) => return TypeId::BIGINT,
            LiteralTypeKind::NotLiteral => {}
        }

        // Unique symbol literal types belong to the symbol family.
        if is_unique_symbol_type(self.ctx.types, type_id) {
            return TypeId::SYMBOL;
        }

        // Check template literals and string intrinsics
        if is_template_literal_type(self.ctx.types, type_id)
            || is_string_intrinsic_type(self.ctx.types, type_id)
        {
            return TypeId::STRING;
        }

        // Intersections narrow their members; if any member sits in a primitive
        // family, treat the intersection as belonging to that family (e.g.
        // `T & number` should count as number-family for TS2367 widening).
        if let Some(list_id) =
            crate::query_boundaries::common::intersection_list_id(self.ctx.types, type_id)
        {
            for member in self.ctx.types.type_list(list_id).iter() {
                let family = self.get_primitive_family(*member);
                if family != TypeId::ERROR {
                    return family;
                }
            }
        }

        TypeId::ERROR // Non-primitive types
    }

    /// Widen types for TS2367 display when they are from different primitive families.
    ///
    /// tsc's rule: when comparing types from different primitive families (e.g., string vs number),
    /// both types are widened to their base primitives in the error message. For same-family
    /// comparisons (e.g., `"foo"` vs `"bar"`), literal types are preserved.
    pub(super) fn widen_for_ts2367_cross_family_display(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> (TypeId, TypeId) {
        let left_family = self.get_primitive_family(left);
        let right_family = self.get_primitive_family(right);

        // Both are primitives, but from different families → widen both
        if left_family != TypeId::ERROR
            && right_family != TypeId::ERROR
            && left_family != right_family
        {
            (
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, left),
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, right),
            )
        } else {
            // Same family (or non-primitives): preserve literal types
            (left, right)
        }
    }

    /// Check a binary operation with `IndexAccess` operands is valid through assignability.
    pub(super) fn resolve_indexed_access_binary_op(
        &mut self,
        left: TypeId,
        right: TypeId,
        op: &str,
    ) -> bool {
        let left_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, left);
        let right_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, right);

        if !left_is_index_access && !right_is_index_access {
            return false;
        }

        match op {
            "+" | "-" | "*" | "/" | "%" | "**" => {
                let left_ok = crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                    self.ctx.types,
                    left,
                )
                    || left_is_index_access && self.is_assignable_to(left, TypeId::NUMBER);
                let right_ok =
                    crate::query_boundaries::type_computation::core::is_arithmetic_operand(
                        self.ctx.types,
                        right,
                    ) || right_is_index_access && self.is_assignable_to(right, TypeId::NUMBER);
                left_ok && right_ok
            }
            _ => false,
        }
    }
}
