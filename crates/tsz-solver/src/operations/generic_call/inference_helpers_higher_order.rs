//! Higher-order target classification and upper-bound resolution helpers.
//!
//! Predicates and small resolvers used during generic call inference to decide
//! whether a higher-order target position may re-generalize through outer
//! inference placeholders, and to pick a single concrete upper bound for an
//! inference variable.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Whether a higher-order target position is built *entirely* from outer
    /// inference placeholders the current resolution may re-generalize through,
    /// carried only by inert structural wrappers (`Array`/tuple).
    ///
    /// A position qualifies (`true`) when it is a bare accepted placeholder
    /// (`__infer_1`) or a wrapper around qualifying children
    /// (`__infer_src_3#X[]`, `[__infer_1, __infer_2]`). The wrapper case arises
    /// in round 2 of a higher-order call: once the shared middle type is fixed
    /// (`B = X[]`), the next generic argument's target parameter becomes
    /// `X_src[]` rather than a bare placeholder. Both forms must route through
    /// the generic-source constraint branch so the surviving placeholder chains
    /// into the result; a nested instantiation against the wrapper drops the
    /// foreign placeholder to `unknown`.
    ///
    /// Returns `false` the moment a non-placeholder concrete leaf (object,
    /// primitive, application, function, …) or an unaccepted placeholder is
    /// reached: a concrete position carries independent inference evidence that
    /// must pin the source parameter by instantiation, so it must not take the
    /// re-generalization path. This keeps the bare-placeholder behavior a strict
    /// subset and the overall check fail-closed. `at_least_one` is set whenever
    /// an accepted placeholder leaf is seen so the caller can reject an
    /// all-empty match.
    pub(super) fn position_is_regeneralizable_higher_order_target(
        &self,
        ty: TypeId,
        at_least_one: &mut bool,
    ) -> bool {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) => {
                // A higher-order source placeholder is always accepted: it is
                // minted within this resolution to carry a generic argument's
                // free param into a later argument's target. A call-local outer
                // placeholder is accepted only when it belongs to THIS call
                // (fail-closed against nested/stale state).
                let accepted = info.origin.is_infer_source()
                    || (info.origin.is_current_infer_placeholder()
                        && self
                            .current_call_inference_placeholders
                            .contains(&info.name));
                if accepted {
                    *at_least_one = true;
                }
                accepted
            }
            Some(TypeData::Array(element)) => {
                self.position_is_regeneralizable_higher_order_target(element, at_least_one)
            }
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.interner.tuple_list(list_id);
                // A tuple qualifies only when it has elements and every element
                // is itself qualifying (no rest spreads, which carry their own
                // array structure with potentially concrete content).
                !elements.is_empty()
                    && elements.iter().all(|element| {
                        !element.rest
                            && self.position_is_regeneralizable_higher_order_target(
                                element.type_id,
                                at_least_one,
                            )
                    })
            }
            _ => false,
        }
    }

    pub(super) fn single_concrete_upper_bound(
        &self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
    ) -> Option<TypeId> {
        let constraints = infer_ctx.get_constraints(var)?;
        let mut concrete_upper_bounds = constraints
            .upper_bounds
            .iter()
            .copied()
            .filter(|upper| {
                !upper.is_any_unknown_or_error()
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *upper,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *upper,
                    )
            })
            .collect::<Vec<_>>();
        concrete_upper_bounds.dedup();
        if concrete_upper_bounds.len() == 1 {
            concrete_upper_bounds.pop()
        } else {
            None
        }
    }
}
