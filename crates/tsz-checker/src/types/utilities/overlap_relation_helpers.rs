//! Relation probes used by diagnostic overlap checks.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn diagnostic_overlap_assignability_directions(
        &mut self,
        left: TypeId,
        right: TypeId,
        skip_signature_only_assignability: bool,
    ) -> (bool, bool) {
        let left_to_right = !skip_signature_only_assignability
            && self.diagnostic_relation_boolean_guard(left, right);
        let right_to_left = if left_to_right || skip_signature_only_assignability {
            false
        } else {
            self.diagnostic_relation_boolean_guard(right, left)
        };

        if tracing::enabled!(tracing::Level::TRACE) {
            let left_type_str = self.format_type(left);
            let right_type_str = self.format_type(right);
            tracing::trace!(
                ?left,
                ?right,
                %left_type_str,
                %right_type_str,
                left_to_right,
                right_to_left,
                "assignability check"
            );
        }

        (left_to_right, right_to_left)
    }

    /// Check if any pair of signatures (one from each side) is related in a
    /// single direction across all shared-arity params and the return type.
    /// Generic signatures (with non-empty `type_params`) are always treated as
    /// comparable to preserve tsc's permissive behavior for constraints that
    /// resolve via apparent types.
    pub(crate) fn any_signatures_comparable(
        &mut self,
        left_sigs: &[tsz_solver::CallSignature],
        right_sigs: &[tsz_solver::CallSignature],
    ) -> bool {
        for lsig in left_sigs {
            let lparams = lsig.params.clone();
            let lret = lsig.return_type;
            let l_is_generic = !lsig.type_params.is_empty();
            for rsig in right_sigs {
                let rparams = rsig.params.clone();
                let rret = rsig.return_type;
                let r_is_generic = !rsig.type_params.is_empty();
                if l_is_generic || r_is_generic {
                    return true;
                }
                let min_pairs = lparams.len().min(rparams.len());
                let mut left_to_right = true;
                let mut right_to_left = true;
                for i in 0..min_pairs {
                    let lp = &lparams[i];
                    let rp = &rparams[i];
                    if lp.optional && rp.optional && !lp.rest && !rp.rest {
                        continue;
                    }
                    let lt = if lp.rest {
                        crate::query_boundaries::common::array_element_type(
                            self.ctx.types,
                            lp.type_id,
                        )
                        .unwrap_or(lp.type_id)
                    } else {
                        lp.type_id
                    };
                    let rt = if rp.rest {
                        crate::query_boundaries::common::array_element_type(
                            self.ctx.types,
                            rp.type_id,
                        )
                        .unwrap_or(rp.type_id)
                    } else {
                        rp.type_id
                    };
                    left_to_right &= self.diagnostic_relation_boolean_guard(lt, rt);
                    right_to_left &= self.diagnostic_relation_boolean_guard(rt, lt);
                    if !left_to_right && !right_to_left {
                        break;
                    }
                }
                if (left_to_right && self.diagnostic_relation_boolean_guard(lret, rret))
                    || (right_to_left && self.diagnostic_relation_boolean_guard(rret, lret))
                {
                    return true;
                }
            }
        }
        false
    }
}
