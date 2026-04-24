use super::FlowAnalyzer;
use crate::query_boundaries::common::union_members;
use crate::query_boundaries::flow as flow_boundary;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{AccessExprData, Node};
use tsz_solver::{GuardSense, NarrowingContext, TypeGuard, TypeId};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn narrow_call_expression_condition(
        &self,
        type_id: TypeId,
        cond_node: &Node,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
    ) -> Option<TypeId> {
        if let Some(call) = self.arena.get_call_expr(cond_node)
            && let Some(node_types) = self.node_types
            && let Some(&callee_type) = node_types.get(&call.expression.0)
            && let Some(signature) = self.predicate_signature_for_type(callee_type)
            && signature.predicate.asserts
            && let Some(narrowed) = self.narrow_by_call_predicate(type_id, call, target, true)
        {
            return Some(narrowed);
        }

        if let Some((guard, guard_target, is_optional)) = self.extract_type_guard(condition_idx) {
            if is_optional && !is_true_branch {
                return Some(type_id);
            }

            if self.is_matching_reference(guard_target, target) {
                return Some(self.apply_call_expression_guard(
                    type_id,
                    cond_node,
                    target,
                    is_true_branch,
                    narrowing,
                    guard,
                ));
            }

            if self.contains_optional_chain(guard_target)
                && self.is_optional_chain_prefix(guard_target, target)
            {
                return Some(flow_boundary::narrow_optional_chain(
                    self.interner.as_type_database(),
                    type_id,
                ));
            }
        }

        let call = self.arena.get_call_expr(cond_node)?;
        if let Some(narrowed) = self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
        {
            return Some(narrowed);
        }
        if is_true_branch {
            let optional_call = cond_node.is_optional_chain();
            if optional_call && self.is_matching_reference(call.expression, target) {
                return Some(flow_boundary::narrow_optional_chain(
                    self.interner.as_type_database(),
                    type_id,
                ));
            }
            if let Some(callee_node) = self.arena.get(call.expression)
                && let Some(access) = self.arena.get_access_expr(callee_node)
                && self.call_access_is_optional_chain(callee_node, access)
                && self.is_matching_reference(access.expression, target)
            {
                return Some(flow_boundary::narrow_optional_chain(
                    self.interner.as_type_database(),
                    type_id,
                ));
            }
        }

        None
    }

    fn apply_call_expression_guard(
        &self,
        type_id: TypeId,
        cond_node: &Node,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        guard: TypeGuard,
    ) -> TypeId {
        use tracing::trace;

        trace!(
            ?guard,
            ?type_id,
            ?is_true_branch,
            "Applying guard from call expression"
        );
        let guard_sense = match guard {
            TypeGuard::Predicate { asserts: true, .. } => GuardSense::Positive,
            _ => GuardSense::from(is_true_branch),
        };
        let result = narrowing.narrow_type(type_id, &guard, guard_sense);
        trace!(?result, "Guard application result");
        if !is_true_branch
            && result == type_id
            && let TypeGuard::Predicate {
                type_id: Some(predicate_type),
                ..
            } = guard
        {
            let positive = narrowing.narrow_type(type_id, &guard, GuardSense::Positive);
            if positive != type_id && positive != TypeId::NEVER {
                let excluded = narrowing.narrow_excluding_type(type_id, positive);
                if excluded != type_id {
                    return excluded;
                }
            }

            let members = union_members(self.interner, type_id).unwrap_or_else(|| vec![type_id]);
            let excluded_members: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|member| self.is_assignable_to(*member, predicate_type))
                .collect();
            if !excluded_members.is_empty() {
                let excluded = narrowing.narrow_excluding_types(type_id, &excluded_members);
                if excluded != type_id {
                    return excluded;
                }
            }
        }
        if result == type_id
            && let Some(call) = self.arena.get_call_expr(cond_node)
            && let Some(retry) =
                self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
            && retry != type_id
        {
            return retry;
        }
        result
    }

    const fn call_access_is_optional_chain(&self, node: &Node, access: &AccessExprData) -> bool {
        access.question_dot_token || node.is_optional_chain()
    }
}
