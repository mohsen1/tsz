use super::FlowAnalyzer;
use crate::query_boundaries::common::{TypeGuard, TypeofKind};
use crate::query_boundaries::flow_analysis::union_members_for_type;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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

    /// Check if `target` is an intermediate segment in an optional chain `chain_node`.
    ///
    /// When a type guard narrows `x?.y?.z`, intermediate segments like `x.y` and `x`
    /// should also be narrowed by removing null/undefined. This is because if
    /// `x?.y?.z` is non-nullish, all intermediate accesses must also be non-nullish.
    ///
    /// Returns `true` if `target` matches any prefix of the optional chain.
    pub(crate) fn is_optional_chain_prefix(
        &self,
        chain_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let chain_node = self.arena.skip_parenthesized_and_assertions(chain_node);
        let Some(node) = self.arena.get(chain_node) else {
            return false;
        };
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            // Check if the base expression matches target
            if self.is_matching_reference(access.expression, target) {
                return true;
            }
            // Also check: does the current chain node (e.g. animal?.breed) match
            // the target (e.g. animal.breed) when ignoring the optional dot?
            // This handles the case where the chain has `?.` but the target uses `.`.
            if self.is_matching_optional_access_reference(chain_node, target) {
                return true;
            }
            // Recurse into the base expression
            return self.is_optional_chain_prefix(access.expression, target);
        }
        false
    }

    /// Match a property/element access reference ignoring `?.` vs `.` differences.
    ///
    /// `is_matching_reference` can't match `x?.y` against `x.y` because
    /// `property_reference` returns `None` for optional chains. This helper
    /// compares the structure directly: same property name and matching base.
    fn is_matching_optional_access_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a = self.arena.skip_parenthesized_and_assertions(a);
        let b = self.arena.skip_parenthesized_and_assertions(b);
        let (Some(node_a), Some(node_b)) = (self.arena.get(a), self.arena.get(b)) else {
            return false;
        };
        // Both must be the same kind of access expression
        if node_a.kind != node_b.kind {
            return false;
        }
        if node_a.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node_a.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let (Some(access_a), Some(access_b)) = (
            self.arena.get_access_expr(node_a),
            self.arena.get_access_expr(node_b),
        ) else {
            return false;
        };
        // Compare property names
        if node_a.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let ident_a = self
                .arena
                .get_identifier_at(access_a.name_or_argument)
                .map(|i| &i.escaped_text);
            let ident_b = self
                .arena
                .get_identifier_at(access_b.name_or_argument)
                .map(|i| &i.escaped_text);
            if ident_a != ident_b || ident_a.is_none() {
                return false;
            }
        } else {
            // Element access - compare using literal values
            let atom_a = self.literal_atom_from_node_or_type(access_a.name_or_argument);
            let atom_b = self.literal_atom_from_node_or_type(access_b.name_or_argument);
            if atom_a != atom_b || atom_a.is_none() {
                return false;
            }
        }
        // Base expressions must match (recursively, also ignoring optional dots)
        self.is_matching_reference(access_a.expression, access_b.expression)
            || self.is_matching_optional_access_reference(access_a.expression, access_b.expression)
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
