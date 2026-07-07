//! Open-ended-spread classification and overload-candidate selection helpers.
//!
//! Split from `candidate_collection`/`overload_resolution` to keep those modules
//! under the per-file line ceiling. Hosts the queries that decide whether a call
//! carries an open-ended (non-tuple) array/iterable spread and, during overload
//! resolution, whether fixed-arity candidates should be skipped so the spread
//! binds to a reachable rest overload (tsc's `hasEffectiveRestParameter`
//! precondition in `chooseOverload`).

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, spread_type_parameter_constraint_is_array_or_tuple_like_for_call,
    tuple_elements_for_type, type_param_variadic_tuple_spread,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Whether the call's arguments contain an open-ended (non-tuple)
    /// array/iterable spread — `...arr` where `arr: T[]` or a custom iterable,
    /// whose runtime length is unknown at the call site.
    ///
    /// Such a spread can only be satisfied by an effective rest parameter, so it
    /// mirrors tsc's `hasEffectiveRestParameter` precondition during overload
    /// resolution: a candidate signature with a fixed parameter list (no rest)
    /// is not applicable to a spread call. Tuple-typed spreads, array-literal
    /// spreads (expanded element-by-element), and type-parameter spreads whose
    /// constraint is array/tuple-like are *not* open-ended — they advance the
    /// argument list by a known count and bind positionally, so they are
    /// excluded here, exactly as in
    /// [`CheckerState::validate_non_tuple_spreads_for_signature`].
    pub(super) fn call_has_open_ended_array_spread_argument(&mut self, args: &[NodeIndex]) -> bool {
        for &arg_idx in args {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            // Tuple-typed spread (fixed positional expansion): not open-ended.
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type)
                && !type_param_variadic_tuple_spread(self.ctx.types, spread_type, &elems)
            {
                continue;
            }
            // Array-literal spread (`...['a', 'b']`) is expanded element-by-element.
            if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                && let Some(expr_node) = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(spread_data.expression))
                && self.ctx.arena.get_literal_expr(expr_node).is_some()
            {
                continue;
            }
            // Type-parameter spread whose constraint is array/tuple-like binds
            // as a single positional unit, not an open-ended overflow.
            if spread_type_parameter_constraint_is_array_or_tuple_like_for_call(
                self.ctx.types,
                spread_type,
                |ty| self.evaluate_type_with_env(ty),
            ) {
                continue;
            }
            let is_open_ended_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if is_open_ended_spread {
                return true;
            }
        }
        false
    }

    /// The number of positional argument slots that precede the first open-ended
    /// (non-tuple) array/iterable spread, expanding earlier tuple/array-literal
    /// spreads by their known element count. This is the parameter index the
    /// spread occupies, used to decide whether a rest overload's trailing rest
    /// parameter actually sits at or before that index (and so can absorb the
    /// spread). Returns the total positional count when there is no open-ended
    /// spread.
    pub(super) fn positional_arg_count_before_open_ended_spread(
        &mut self,
        args: &[NodeIndex],
    ) -> usize {
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
            // Tuple-typed spread expands by its element count.
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type)
                && !type_param_variadic_tuple_spread(self.ctx.types, spread_type, &elems)
            {
                effective_index += elems.len();
                continue;
            }
            // Array-literal spread expands by its literal element count.
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
            // Type-parameter spread with array/tuple-like constraint occupies one slot.
            if spread_type_parameter_constraint_is_array_or_tuple_like_for_call(
                self.ctx.types,
                spread_type,
                |ty| self.evaluate_type_with_env(ty),
            ) {
                effective_index += 1;
                continue;
            }
            let is_open_ended_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if is_open_ended_spread {
                return effective_index;
            }
            effective_index += 1;
        }
        effective_index
    }

    /// Whether, during overload resolution, fixed-arity (non-rest) candidates
    /// should be skipped because the call carries an open-ended (non-tuple)
    /// array/iterable spread that can only land on an effective rest parameter
    /// (tsc's `hasEffectiveRestParameter` precondition in `chooseOverload`).
    ///
    /// A fixed-arity overload would otherwise win on the collapsed single-`any`
    /// argument count and then emit a spurious TS2556. Skipping non-rest
    /// candidates lets the spread bind to the rest overload, matching tsc.
    ///
    /// The skip is gated on a *reachable* rest overload: one whose trailing rest
    /// parameter sits at or before the position the spread occupies (`rest index
    /// <= leading positional argument count`). Without that gate, a spread that
    /// lands on a fixed parameter of every overload (e.g. a rest overload
    /// `(a, b, c, ...rest)` called as `f(x, ...arr)`, where the spread hits `b`)
    /// would have its non-rest siblings skipped yet the rest overload also
    /// rejected on arity, suppressing the diagnostic the spread genuinely
    /// deserves. When no rest overload is reachable, the loops run unchanged.
    pub(super) fn skip_non_rest_overloads_for_open_ended_spread(
        &mut self,
        args: &[NodeIndex],
        signatures: &[tsz_solver::CallSignature],
    ) -> bool {
        if !self.call_has_open_ended_array_spread_argument(args) {
            return false;
        }
        let leading_positional_arg_count = self.positional_arg_count_before_open_ended_spread(args);
        signatures.iter().any(|sig| {
            sig.params
                .iter()
                .position(|param| param.rest)
                .is_some_and(|rest_index| rest_index <= leading_positional_arg_count)
        })
    }

    /// The most recent open-ended (non-tuple) array/iterable spread argument
    /// that precedes the positional `mismatch_index`, if any. Used to decide
    /// whether a downstream argument-type mismatch should instead surface as a
    /// TS2556 against that earlier spread (whose unknown length pushed later
    /// arguments onto the wrong parameters). Tuple-typed and array-literal
    /// spreads are skipped by their known element count and never recorded —
    /// only opaque spreads with indeterminate runtime length qualify.
    pub(super) fn find_prior_non_tuple_spread_for_mismatch(
        &mut self,
        args: &[NodeIndex],
        mismatch_index: usize,
    ) -> Option<NodeIndex> {
        let mut effective_index = 0usize;
        let mut prior_non_tuple_spread = None;

        for &arg_idx in args {
            if effective_index > mismatch_index {
                break;
            }
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                if effective_index == mismatch_index {
                    return prior_non_tuple_spread;
                }
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            // A variadic-tuple type-parameter spread stays a single unit (see argument
            // collection); treat it as one position rather than its constraint's
            // tuple element count.
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type)
                && !type_param_variadic_tuple_spread(self.ctx.types, spread_type, &elems)
            {
                if mismatch_index < effective_index + elems.len() {
                    return prior_non_tuple_spread;
                }
                effective_index += elems.len();
                continue;
            }
            // An array literal spread (e.g. `...['a', 'x']`) is expanded element-by-element
            // during argument collection. A mismatch at one of those expanded indices is a
            // per-element type error (TS2345/TS2322), not a TS2556. Skip past the literal's
            // elements without setting `prior_non_tuple_spread`.
            if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                && let Some(expr_node) = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(spread_data.expression))
                && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
            {
                let count = literal.elements.nodes.len();
                if mismatch_index < effective_index + count {
                    return prior_non_tuple_spread;
                }
                effective_index += count;
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if effective_index == mismatch_index {
                return prior_non_tuple_spread;
            }
            if is_non_tuple_spread {
                prior_non_tuple_spread = Some(arg_idx);
            }
            effective_index += 1;
        }

        prior_non_tuple_spread
    }
}
