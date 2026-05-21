use super::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::union_members_for_type;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::narrowing::{TypeGuard, TypeofKind};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn contains_optional_chain(&self, idx: NodeIndex) -> bool {
        let idx = self.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if node.is_optional_chain() {
                return true;
            }
            return self.contains_optional_chain(call.expression);
        }
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if self.access_expr_is_optional_chain(node, access) {
                return true;
            }
            return self.contains_optional_chain(access.expression);
        }
        false
    }

    pub(super) const fn access_expr_is_optional_chain(
        &self,
        node: &tsz_parser::parser::node::Node,
        access: &tsz_parser::parser::node::AccessExprData,
    ) -> bool {
        access.question_dot_token || node.is_optional_chain()
    }

    pub(crate) fn is_optional_chain_containing_target(
        &self,
        expr: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let expr = self.arena.skip_parenthesized_and_assertions(expr);
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            if let Some(unary) = self.arena.get_unary_expr(node)
                && unary.operator == SyntaxKind::TypeOfKeyword as u16
            {
                return self.is_optional_chain_containing_target(unary.operand, target);
            }
            return false;
        }
        if !self.contains_optional_chain(expr) {
            return false;
        }
        if self.is_optional_chain_prefix(expr, target) {
            return true;
        }

        let mut cur = expr;
        for _ in 0..64 {
            if self.is_matching_reference(cur, target) {
                return true;
            }
            let Some(cur_node) = self.arena.get(cur) else {
                return false;
            };
            if cur_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.arena.get_call_expr(cur_node)
            {
                cur = self
                    .arena
                    .skip_parenthesized_and_assertions(call.expression);
                continue;
            }
            if (cur_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || cur_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.arena.get_access_expr(cur_node)
            {
                cur = self
                    .arena
                    .skip_parenthesized_and_assertions(access.expression);
                continue;
            }
            return false;
        }
        false
    }

    pub(super) const fn optional_chain_guard_can_be_satisfied_by_short_circuit(
        &self,
        guard: &TypeGuard,
    ) -> bool {
        matches!(
            guard,
            TypeGuard::NullishEquality
                | TypeGuard::LiteralEquality(TypeId::UNDEFINED)
                | TypeGuard::Typeof(TypeofKind::Undefined)
                | TypeGuard::Discriminant {
                    value_type: TypeId::UNDEFINED,
                    ..
                }
        )
    }

    pub(super) fn optional_chain_comparison_proves_non_nullish(
        &self,
        bin: &BinaryExprData,
        target: NodeIndex,
        is_strict: bool,
        effective_truth: bool,
    ) -> bool {
        let Some(node_types) = self.node_types else {
            return false;
        };

        for (chain_side, other_side) in [(bin.left, bin.right), (bin.right, bin.left)] {
            if !self.is_optional_chain_containing_target(chain_side, target) {
                continue;
            }
            if self.typeof_optional_chain_short_circuit_matches_literal(chain_side, other_side) {
                if !effective_truth {
                    return true;
                }
                continue;
            }
            if self.value_optional_chain_short_circuit_matches_literal(other_side, is_strict) {
                if !effective_truth {
                    return true;
                }
                continue;
            }
            if !effective_truth {
                continue;
            }
            let Some(&other_type) = node_types.get(&other_side.0) else {
                continue;
            };
            if !self.comparison_allows_optional_chain_short_circuit(other_type, is_strict) {
                return true;
            }
        }

        false
    }

    fn typeof_optional_chain_short_circuit_matches_literal(
        &self,
        chain_side: NodeIndex,
        other_side: NodeIndex,
    ) -> bool {
        let Some(typeof_operand) = self.get_typeof_operand(self.skip_parenthesized(chain_side))
        else {
            return false;
        };
        self.contains_optional_chain(typeof_operand)
            && self.literal_string_from_node(other_side) == Some("undefined")
    }

    fn value_optional_chain_short_circuit_matches_literal(
        &self,
        other_side: NodeIndex,
        is_strict: bool,
    ) -> bool {
        match self.literal_type_from_node(other_side) {
            Some(TypeId::UNDEFINED) => true,
            Some(TypeId::NULL) => !is_strict,
            _ => false,
        }
    }

    fn comparison_allows_optional_chain_short_circuit(
        &self,
        compared_type: TypeId,
        is_strict: bool,
    ) -> bool {
        if compared_type.is_any_or_unknown() || compared_type == TypeId::ERROR {
            return true;
        }
        self.type_contains(compared_type, TypeId::UNDEFINED)
            || (!is_strict && self.type_contains(compared_type, TypeId::NULL))
    }

    fn type_contains(&self, type_id: TypeId, needle: TypeId) -> bool {
        if type_id == needle {
            return true;
        }
        union_members_for_type(self.interner, type_id)
            .map(|members| {
                members
                    .into_iter()
                    .any(|member| self.type_contains(member, needle))
            })
            .unwrap_or(false)
    }
}
