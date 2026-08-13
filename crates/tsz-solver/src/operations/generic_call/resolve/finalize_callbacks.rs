//! Callback-position classification and the annotated-parameter guard used by
//! generic-call finalization (issue #17282). Split out of `finalize.rs` to keep
//! that module under the solver's per-file line ceiling.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{FunctionShape, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

use super::CallbackPositionVars;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Classify each type-parameter inference var by the variance of its
    /// occurrences in the call's context-sensitive callback parameters:
    /// `param_only` (contravariant callback-parameter positions only) and
    /// `return_position` (any covariant callback-return position). See
    /// [`CallbackPositionVars`].
    ///
    /// Evaluation of indexed-access/alias callback signatures (e.g.
    /// `Ord<A>['compare']`) is performed here, after argument inference has
    /// settled, so it cannot perturb in-flight inference.
    pub(super) fn callback_position_inference_vars(
        &mut self,
        func: &FunctionShape,
        placeholder_subst: &TypeSubstitution,
        var_map: &FxHashMap<TypeId, InferenceVar>,
    ) -> CallbackPositionVars {
        let mut param_position: FxHashSet<InferenceVar> = FxHashSet::default();
        let mut return_position: FxHashSet<InferenceVar> = FxHashSet::default();
        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        for param in &func.params {
            let instantiated = instantiate_type(self.interner, param.type_id, placeholder_subst);
            // Prefer the raw signature; only fall back to evaluation (which can
            // resolve indexed-access/alias callbacks) when the raw type does not
            // already expose a function shape.
            let shape =
                Self::get_contextual_signature_cached(self.interner, instantiated).or_else(|| {
                    let evaluated = self.checker.evaluate_type(instantiated);
                    (evaluated != instantiated)
                        .then(|| Self::get_contextual_signature_cached(self.interner, evaluated))
                        .flatten()
                });
            let Some(shape) = shape else {
                continue;
            };
            for callback_param in &shape.params {
                visited.clear();
                param_position.extend(self.collect_direct_placeholder_vars_in_type(
                    callback_param.type_id,
                    var_map,
                    &mut visited,
                ));
            }
            visited.clear();
            return_position.extend(self.collect_direct_placeholder_vars_in_type(
                shape.return_type,
                var_map,
                &mut visited,
            ));
        }
        let param_only = param_position
            .difference(&return_position)
            .copied()
            .collect();
        CallbackPositionVars {
            param_only,
            return_position,
        }
    }

    /// Whether the Round-1 fix `fixed` for `var` should be restored over the
    /// re-derived `ty` — tsc's immutable `InferenceInfo.isFixed`, observed at
    /// finalization (issue #17282).
    ///
    /// The rule is applied *after* inference rather than by blocking Round-2
    /// candidate collection: freezing at collection time (the literal tsc
    /// `isFixed`) was tried and rejected because it destroys `any`-propagation
    /// across a second callback and mis-anchors the resulting diagnostic.
    /// Observing the widening and restoring here preserves both.
    ///
    /// Only a body-only covariant widening is restored; the rule stands down in
    /// the cases tsc keeps the widened inference:
    /// - `any_tainted_frozen_call` — an `any`/`unknown` result on *any* frozen
    ///   return-position variable disables restore for the whole call, not just
    ///   that variable: the propagated `any` tsc would collapse the parameters
    ///   with cannot cross the second callback's parameter in a two-round model,
    ///   so a sibling variable that re-derived to a concrete type must still be
    ///   left widened;
    /// - [`Self::contra_candidate_diverges_from_fixed`] — an annotated callback
    ///   parameter supplied a divergent type.
    pub(super) fn should_restore_round1_fix(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
        fixed: TypeId,
        ty: TypeId,
        callback_return_position_vars: &FxHashSet<InferenceVar>,
        any_tainted_frozen_call: bool,
    ) -> bool {
        !any_tainted_frozen_call
            && callback_return_position_vars.contains(&var)
            && fixed != ty
            && !ty.is_any_unknown_or_error()
            && self.checker.is_assignable_to(fixed, ty)
            && !self.contra_candidate_diverges_from_fixed(infer_ctx, var, fixed)
    }

    /// Whether `var` has a usable contravariant candidate that is *not*
    /// type-equivalent to its Round-1 fix `fixed`.
    ///
    /// A divergent contra candidate is real inference evidence contributed by an
    /// explicitly annotated callback parameter (e.g. `(acc: number[], e) => …`
    /// against a `never[]` fix); the widened `ty` it produces is what tsc keeps,
    /// so the `#17282` restore must stand down. A contra candidate equal to the
    /// fix — the un-annotated swapped callback parameter, contextually typed to
    /// the fix itself — carries no new information, so the restore proceeds.
    pub(super) fn contra_candidate_diverges_from_fixed(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
        fixed: TypeId,
    ) -> bool {
        infer_ctx
            .get_contra_candidate_types(var)
            .into_iter()
            .any(|contra| {
                if contra.is_any_unknown_or_error() {
                    return false;
                }
                // Deliberately the raw bidirectional relation, not
                // `are_types_identical`: the checker's override resolves `Lazy`
                // operands through the type environment and reports two
                // structurally-equal interfaces as distinct here, which would
                // treat a redundant contextual contra candidate as divergent and
                // suppress the #17282 restore.
                let equivalent_to_fix = self.checker.is_assignable_to(contra, fixed)
                    && self.checker.is_assignable_to(fixed, contra);
                !equivalent_to_fix
            })
    }
}
