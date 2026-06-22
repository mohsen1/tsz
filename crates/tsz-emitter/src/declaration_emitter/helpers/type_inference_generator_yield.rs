//! Generator yield-type inference helpers for declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// A single `yield` operand contribution to a generator's inferred yield type.
struct YieldOperand {
    /// The operand's declaration-emit type (fresh object/array operands already
    /// have their member positions widened).
    type_id: tsz_solver::types::TypeId,
    /// Whether the operand is a fresh primitive literal (a bare literal token,
    /// not a const/type assertion). A *single* fresh literal widens to its base;
    /// a union of distinct literals is preserved.
    is_fresh_literal: bool,
}

impl DeclarationEmitter<'_> {
    pub(in crate::declaration_emitter) fn generator_yield_return_type_text(
        &self,
        is_async: bool,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let generator_name = if is_async {
            "AsyncGenerator"
        } else {
            "Generator"
        };

        let mut operands: Vec<YieldOperand> = Vec::new();
        if !self.collect_generator_yield_operands(body_idx, is_async, &mut operands, 0) {
            return None;
        }

        // Empty generators produce no values; bare `yield;` is collected as
        // `undefined` and therefore does not reach this branch.
        if operands.is_empty() {
            return Some(format!("{generator_name}<never, void, unknown>"));
        }

        // Match tsc's `getWidenedType(getUnionType(<yield operand types>))`:
        // preserve distinct literal unions, but widen one fresh literal value.
        let any_fresh_literal = operands.iter().any(|operand| operand.is_fresh_literal);
        let union_ty =
            interner.union_literal_reduce(operands.iter().map(|operand| operand.type_id).collect());
        let yield_ty =
            if any_fresh_literal && tsz_solver::query::is_literal_type(interner, union_ty) {
                tsz_solver::computation::widen_literal_type(interner, union_ty)
            } else {
                union_ty
            };

        let yield_text = self.print_type_id_for_inferred_declaration(yield_ty);
        if yield_text.is_empty() || yield_text == "any" {
            return None;
        }
        Some(format!("{generator_name}<{yield_text}, void, unknown>"))
    }

    /// Collect the type of each `yield` operand in `node_idx`'s body. Nested
    /// functions/classes are skipped because their yields belong to another
    /// generator scope.
    fn collect_generator_yield_operands(
        &self,
        node_idx: NodeIndex,
        is_async: bool,
        operands: &mut Vec<YieldOperand>,
        depth: usize,
    ) -> bool {
        if node_idx.is_none() || depth > 128 {
            return true;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return true;
        };

        match node.kind {
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                let Some(yield_expr) = self.arena.get_unary_expr_ex(node) else {
                    return false;
                };
                if yield_expr.asterisk_token {
                    let Some(element) =
                        self.yield_star_delegated_yield_type_id(yield_expr.expression, is_async)
                    else {
                        return false;
                    };
                    operands.push(YieldOperand {
                        type_id: element,
                        is_fresh_literal: false,
                    });
                    return true;
                }
                if yield_expr.expression.is_none() {
                    operands.push(YieldOperand {
                        type_id: tsz_solver::types::TypeId::UNDEFINED,
                        is_fresh_literal: false,
                    });
                    return true;
                }
                match self.yield_operand_type(yield_expr.expression) {
                    Some(operand) => {
                        operands.push(operand);
                        true
                    }
                    None => false,
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR =>
            {
                true
            }
            _ => self
                .arena
                .get_children(node_idx)
                .into_iter()
                .all(|child_idx| {
                    self.collect_generator_yield_operands(child_idx, is_async, operands, depth + 1)
                }),
        }
    }

    /// Resolve a single non-`yield*`, non-empty `yield` operand to its
    /// declaration-emit type.
    fn yield_operand_type(&self, expr_idx: NodeIndex) -> Option<YieldOperand> {
        let interner = self.type_interner?;
        let asserted = self.explicit_asserted_type_text(expr_idx).is_some();
        let inner_idx = self
            .skip_parenthesized_expression(expr_idx)
            .unwrap_or(expr_idx);

        // The checker stores widened types at yield sites, so reconstruct fresh
        // primitive literals from syntax before the operand union is built.
        if !asserted && let Some(literal_ty) = self.yield_literal_operand_type_id(inner_idx) {
            return Some(YieldOperand {
                type_id: literal_ty,
                is_fresh_literal: true,
            });
        }

        let type_id = self.get_node_type_or_names(&[expr_idx])?;
        if type_id == tsz_solver::types::TypeId::ANY || type_id == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }
        let type_id = self.widen_unique_symbol_value_type_for_dts(type_id, 0);

        let is_fresh_compound = self.arena.get(inner_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        });
        let type_id = if is_fresh_compound {
            tsz_solver::computation::widen_literal_type(interner, type_id)
        } else {
            type_id
        };

        Some(YieldOperand {
            type_id,
            is_fresh_literal: false,
        })
    }

    fn yield_literal_operand_type_id(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let node = self.arena.get(node_idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(node)?;
                Some(interner.literal_string(&literal.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let literal = self.arena.get_literal(node)?;
                let value = literal
                    .value
                    .or_else(|| literal.text.replace('_', "").parse::<f64>().ok())?;
                Some(interner.literal_number(value))
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                let literal = self.arena.get_literal(node)?;
                Some(interner.literal_bigint(&literal.text.replace('_', "")))
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some(interner.literal_boolean(true)),
            k if k == SyntaxKind::FalseKeyword as u16 => Some(interner.literal_boolean(false)),
            k if k == SyntaxKind::NullKeyword as u16 => Some(tsz_solver::types::TypeId::NULL),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(node) =>
            {
                let prefix = self.arena.get_unary_expr(node)?;
                let operand = self.arena.get(prefix.operand)?;
                let literal = self.arena.get_literal(operand)?;
                if operand.kind == SyntaxKind::BigIntLiteral as u16 {
                    Some(interner.literal_bigint_with_sign(true, &literal.text.replace('_', "")))
                } else {
                    let value = literal
                        .value
                        .or_else(|| literal.text.replace('_', "").parse::<f64>().ok())?;
                    Some(interner.literal_number(-value))
                }
            }
            _ => None,
        }
    }

    /// Resolve the element type delegated by a `yield* <iterable>` expression.
    fn yield_star_delegated_yield_type_id(
        &self,
        expression: NodeIndex,
        is_async: bool,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let iterable_type = self.get_node_type_or_names(&[expression])?;
        let element = if is_async {
            tsz_solver::computation::get_async_iterable_element_type(interner, iterable_type)
        } else {
            tsz_solver::computation::get_iterator_info(interner, iterable_type, false)
                .map(|info| info.yield_type)?
        };
        let element = self.widen_unique_symbol_value_type_for_dts(element, 0);
        if element == tsz_solver::types::TypeId::ANY || element == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }
        Some(element)
    }
}
