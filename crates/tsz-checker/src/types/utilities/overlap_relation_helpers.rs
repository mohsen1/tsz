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
            && self
                .diagnostic_overlap_relation_outcome(left, right)
                .related;
        let right_to_left = if left_to_right || skip_signature_only_assignability {
            false
        } else {
            self.diagnostic_overlap_relation_outcome(right, left)
                .related
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

    /// Rewrite an enum operand to its member-value domain for TS2367 overlap,
    /// but only when the *other* operand is not itself an enum.
    ///
    /// tsc relates an enum to a primitive/literal through its member *values*:
    /// `Color === "red"` overlaps because `"red"` is a member value, while
    /// `Color === "blue"` reports TS2367. It treats two distinct enums as
    /// nominal, though — `Color === Hue` reports TS2367 even when both declare a
    /// `"red"` member — and the nominal enum-vs-enum cases are already handled by
    /// the assignability fallback. A *single* enum member already unwraps to its
    /// literal in `classify_simple_overlap_type` (so `Color.Red === "red"` is
    /// correct); this covers the whole-enum (member-union) operand the same way,
    /// fixing the string/const-enum-vs-matching-member-literal false positive.
    ///
    /// Returns `Some(no_overlap)` when a value-based rewrite applies (recursing
    /// through `types_have_no_overlap`), or `None` to fall through to the nominal
    /// assignability path.
    pub(super) fn enum_value_overlap_rewrite(
        &mut self,
        left: TypeId,
        right: TypeId,
    ) -> Option<bool> {
        use crate::query_boundaries::flow_analysis::enum_member_domain;

        // `enum_member_domain` returns the enum's member-value union for an enum
        // type and the type unchanged otherwise, so `domain != ty` is exactly
        // "ty is an enum". Rewrite only when one side is an enum and the other is
        // not; that asymmetry also guarantees the recursion makes progress.
        let left_domain = enum_member_domain(self.ctx.types, left);
        let right_domain = enum_member_domain(self.ctx.types, right);
        let left_is_enum = left_domain != left;
        let right_is_enum = right_domain != right;
        if left_is_enum == right_is_enum {
            return None;
        }
        Some(self.types_have_no_overlap(left_domain, right_domain))
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
                    left_to_right &= self.diagnostic_overlap_relation_outcome(lt, rt).related;
                    right_to_left &= self.diagnostic_overlap_relation_outcome(rt, lt).related;
                    if !left_to_right && !right_to_left {
                        break;
                    }
                }
                if (left_to_right && self.diagnostic_overlap_relation_outcome(lret, rret).related)
                    || (right_to_left
                        && self.diagnostic_overlap_relation_outcome(rret, lret).related)
                {
                    return true;
                }
            }
        }
        false
    }
}
