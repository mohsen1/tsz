use crate::call_checker::OverloadResolution;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call::CallResult;
use crate::query_boundaries::construct_signatures::{
    construct_signatures_for_type_with_resolver, reorder_construct_overload_candidates,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::complex::is_contextually_sensitive;

impl<'a> CheckerState<'a> {
    /// Contextually type an ambiguous construct-overload argument list through
    /// the shared per-candidate call engine.
    ///
    /// TypeScript resolves `new` overloads through the same candidate walk as
    /// calls: non-sensitive arguments select a viable candidate before a
    /// callback is permanently contextually typed. A successful walk returns
    /// its selected result so the caller does not run an independent second
    /// resolution over the already-contextualized arguments.
    pub(crate) fn contextual_construct_overload_arg_types(
        &mut self,
        constructor_type: TypeId,
        args: &[NodeIndex],
        contextual_type: Option<TypeId>,
    ) -> Option<OverloadResolution> {
        if !args
            .iter()
            .copied()
            .any(|arg| is_contextually_sensitive(self, arg))
        {
            return None;
        }

        let signatures = construct_signatures_for_type_with_resolver(
            self.ctx.types,
            &self.ctx,
            constructor_type,
        )?;
        if signatures.len() < 2 {
            return None;
        }
        let signatures = reorder_construct_overload_candidates(&signatures);

        let snapshot = self.ctx.snapshot_return_type();
        let resolution = self.resolve_overloaded_call_with_signatures(
            args,
            &signatures,
            false,
            contextual_type,
            None,
        );
        if let Some(resolution) = resolution
            && matches!(&resolution.result, CallResult::Success(_))
        {
            return Some(resolution);
        }

        snapshot.rollback(&mut self.ctx.speculation_state());
        None
    }

    pub(crate) fn generic_new_argument_accepts_contextual_parameter(
        &mut self,
        arg_idx: NodeIndex,
        expected: TypeId,
    ) -> bool {
        if expected == TypeId::ANY || expected == TypeId::ERROR || expected == TypeId::UNKNOWN {
            return false;
        }

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        if !matches!(
            arg_node.kind,
            tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                | tsz_parser::parser::syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) {
            return false;
        }

        let request = TypingRequest::with_contextual_type(expected);
        let contextual_actual = self.speculative_type_of_node(arg_idx, &request);

        contextual_actual != TypeId::ANY
            && contextual_actual != TypeId::ERROR
            && self
                .call_arg_relation_outcome(contextual_actual, expected)
                .related
    }

    pub(crate) fn recover_new_expression_return_type_after_contextual_argument_match(
        &mut self,
        constructor_type: TypeId,
        fallback_return: TypeId,
    ) -> TypeId {
        if fallback_return != TypeId::ERROR {
            fallback_return
        } else {
            self.instance_type_from_constructor_type(constructor_type)
                .unwrap_or(TypeId::ERROR)
        }
    }
}
