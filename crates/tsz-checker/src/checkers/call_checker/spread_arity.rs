//! Spread-argument arity helpers for call checking.
//!
//! Split from `candidate_collection` to keep that module under the per-file
//! line ceiling. Hosts the open-ended-tuple-spread TS2556 suppression query.

use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether a non-tuple array/iterable spread covering positional argument
    /// `index` lands on a rest parameter (or an optional trailing slot) of
    /// `callable_type` — the gate for TS2556.
    ///
    /// When `union_is_overload_set` is set, `callable_type` is the synthetic
    /// `sigA | sigB | ...` union built for the overload-resolution first pass; a
    /// union member is then treated as an *alternative* (existential), so the
    /// spread is admitted when any overload has a rest at `index`. A genuine
    /// union-typed value keeps the conjunctive semantics (all members must
    /// admit the spread).
    pub(crate) fn spread_lands_on_rest_position(
        &self,
        callable_type: TypeId,
        index: usize,
        union_is_overload_set: bool,
    ) -> bool {
        let ctx = ContextualTypeContext::with_expected(self.ctx.types, callable_type);
        if union_is_overload_set {
            ctx.allows_non_tuple_spread_position_existential_union(index)
        } else {
            ctx.allows_non_tuple_spread_position(index)
        }
    }

    /// Whether the open-ended tuple spread `arg_idx`, whose variable rest lands
    /// at positional argument index `variable_index`, targets a callee whose
    /// parameter at that position tsc contextually types from the spread — in
    /// which case TS2556 must be suppressed.
    ///
    /// tsc applies the `restTuplesFromContextualTypes` behaviour when the callee
    /// is an inline function/arrow expression and the parameter sitting at the
    /// variable-rest position is **un-annotated**: that parameter is then
    /// contextually typed from the tuple's rest element and absorbs the variable
    /// portion, so the call is valid. When that parameter is annotated it has a
    /// fixed type the open-ended spread cannot satisfy, and when no parameter
    /// exists at that position the rest overflows the parameter list — TS2556
    /// still fires in both cases, exactly as for a declared function. The callee
    /// is found by walking the parent chain from the argument to the nearest
    /// enclosing call/new expression, so no call-node context has to be threaded
    /// through the argument collector.
    pub(crate) fn spread_callee_infers_params_from_arguments(
        &self,
        arg_idx: NodeIndex,
        variable_index: usize,
    ) -> bool {
        // Walk up to the enclosing call/new expression that owns this argument.
        let mut child = arg_idx;
        let mut callee = NodeIndex::NONE;
        for _ in 0..8 {
            let Some(parent) = self.ctx.arena.parent_of(child) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if (parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::NEW_EXPRESSION)
                && let Some(call) = self.ctx.arena.get_call_expr(parent_node)
            {
                // `child` is on the argument side, never the callee side.
                if call.expression == child {
                    return false;
                }
                callee = call.expression;
                break;
            }
            child = parent;
        }
        if callee.is_none() {
            return false;
        }

        let callee = self.ctx.arena.skip_parenthesized(callee);
        let Some(callee_node) = self.ctx.arena.get(callee) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(callee_node) else {
            return false;
        };
        // The parameter at the variable-rest position must exist and be
        // un-annotated for tsc to contextually type it from the rest element.
        func.parameters
            .nodes
            .get(variable_index)
            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
            .and_then(|node| self.ctx.arena.get_parameter(node))
            .is_some_and(|param| param.type_annotation.is_none())
    }
}
