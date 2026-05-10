//! Feedback from contextual return bounds into higher-order argument inference.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{InferencePriority, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn propagate_contextual_return_upper_bounds(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_return: TypeId,
        target_return: TypeId,
    ) {
        let mut placeholder_probe_map = FxHashMap::default();
        let mut placeholder_visited = FxHashSet::default();
        let target_return_vars = self.collect_placeholder_vars_in_type(
            target_return,
            var_map,
            &mut placeholder_probe_map,
            &mut placeholder_visited,
        );

        for var in target_return_vars {
            let Some(constraints) = infer_ctx.get_constraints(var) else {
                continue;
            };
            for upper in constraints.upper_bounds {
                if upper.is_any_unknown_or_error() {
                    continue;
                }
                placeholder_visited.clear();
                if self.type_contains_placeholder(upper, var_map, &mut placeholder_visited) {
                    continue;
                }

                let nested_structural = self.constrain_return_context_structure(
                    infer_ctx,
                    var_map,
                    upper,
                    source_return,
                    InferencePriority::ReturnType,
                );
                if !nested_structural {
                    self.constrain_types(
                        infer_ctx,
                        var_map,
                        upper,
                        source_return,
                        InferencePriority::ReturnType,
                    );
                }
            }
        }
    }
}
