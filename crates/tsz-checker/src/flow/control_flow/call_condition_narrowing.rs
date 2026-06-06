use super::FlowAnalyzer;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::flow_analysis as flow_query;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{AccessExprData, Node};
use tsz_solver::TypeId;
use tsz_solver::narrowing::{NarrowingContext, TypeGuard};

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

            if let Some(narrowed) = self.narrow_receiver_property_by_predicate(
                type_id,
                guard_target,
                target,
                is_true_branch,
                &guard,
            ) {
                return Some(narrowed);
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

    fn narrow_receiver_property_by_predicate(
        &self,
        type_id: TypeId,
        guard_target: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        guard: &TypeGuard,
    ) -> Option<TypeId> {
        if !is_true_branch {
            return None;
        }
        let TypeGuard::Predicate {
            type_id: Some(predicate_type),
            ..
        } = *guard
        else {
            return None;
        };
        let (target_base, property_name) = self.property_reference(target)?;
        if !self.is_matching_reference(guard_target, target_base) {
            return None;
        }

        let property_name = self.interner.resolve_atom_ref(property_name);
        let predicate_property_type = flow_query::property_type_for_contextual_type(
            self.interner,
            predicate_type,
            property_name.as_ref(),
        )?;
        if matches!(
            predicate_property_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
        ) {
            return None;
        }

        let env = self.type_environment.map(std::cell::RefCell::borrow);
        let narrowed = flow_query::narrow_property_type_by_predicate(
            self.interner,
            env.as_deref(),
            type_id,
            predicate_property_type,
        );
        (narrowed != type_id).then_some(narrowed)
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
        let env = self.type_environment.map(std::cell::RefCell::borrow);
        let result = flow_query::narrow_call_predicate_guard(
            self.interner,
            env.as_deref(),
            self.concrete_this_type,
            narrowing,
            type_id,
            &guard,
            is_true_branch,
        );
        trace!(?result, "Guard application result");
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
