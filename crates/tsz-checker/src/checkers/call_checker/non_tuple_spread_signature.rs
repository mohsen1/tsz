//! Non-tuple spread validation against selected call signatures.

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, spread_type_parameter_constraint_is_array_or_tuple_like_for_call,
    tuple_elements_for_type, type_param_variadic_tuple_spread,
};
use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn validate_non_tuple_spreads_for_signature(
        &mut self,
        args: &[NodeIndex],
        func_type: TypeId,
    ) {
        if let Some(arg_idx) = self.first_non_tuple_spread_rejected_by_signature(args, func_type) {
            self.error_spread_must_be_tuple_or_rest_at(arg_idx);
        }
    }

    /// Like [`Self::validate_non_tuple_spreads_for_signature`] but returns the
    /// first spread argument that `func_type` cannot absorb at a rest (or
    /// optional-tail) position, without emitting a diagnostic. Overload
    /// resolution uses this to treat a non-tuple spread that overflows a
    /// fixed-arity overload as a *soft* mismatch (try the next overload) rather
    /// than committing TS2556 — a sibling overload with a rest parameter may
    /// absorb the spread (#14319).
    pub(super) fn first_non_tuple_spread_rejected_by_signature(
        &mut self,
        args: &[NodeIndex],
        func_type: TypeId,
    ) -> Option<NodeIndex> {
        let ctx = ContextualTypeContext::with_expected(self.ctx.types, func_type);
        let mut effective_index = 0usize;
        for &arg_idx in args {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            // A scalar `any`/`error` spread is assignable to any rest or fixed
            // parameter and can never overflow the parameter list, so it is
            // never a rejectable non-tuple spread. This mirrors the collector
            // early-out in `collect_call_argument_types_with_context`; keeping
            // the exemption in this shared overload predicate ensures the
            // single-signature and overload-resolution paths agree (the value
            // occupies one positional slot here).
            if spread_type == TypeId::ANY || spread_type == TypeId::ERROR {
                effective_index += 1;
                continue;
            }
            // A variadic-tuple type-parameter spread stays a single unit (see argument
            // collection and the type-parameter spread branch below); do not
            // advance by its constraint's tuple element count.
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type)
                && !type_param_variadic_tuple_spread(self.ctx.types, spread_type, &elems)
            {
                effective_index += elems.len();
                continue;
            }
            // An array literal spread (e.g. `...['a', 'x']`) is expanded element-by-element
            // during argument collection, so each element is checked individually against
            // the corresponding parameter. Treat it like a tuple-like spread here: advance
            // by the literal's element count and skip the TS2556 emission. tsc behaves the
            // same way — TS2556 is only reported for spreads of opaque arrays/iterables
            // whose runtime length is unknown at the call site.
            if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                && let Some(expr_node) = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(spread_data.expression))
                && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
            {
                effective_index += literal.elements.nodes.len();
                continue;
            }
            if spread_type_parameter_constraint_is_array_or_tuple_like_for_call(
                self.ctx.types,
                spread_type,
                |ty| self.evaluate_type_with_env(ty),
            ) {
                effective_index += 1;
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if is_non_tuple_spread && !ctx.allows_non_tuple_spread_position(effective_index) {
                return Some(arg_idx);
            }
            effective_index += 1;
        }
        None
    }
}
