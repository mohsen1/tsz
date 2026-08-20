//! Per-property elaboration target for an object literal against a union
//! target.
//!
//! `tsc`'s `elaborateElementwise` derives each property's target through
//! `getBestMatchIndexedAccessTypeOrUndefined`: the first step
//! (`getIndexedAccessTypeOrUndefined`) is defined on a union only when EVERY
//! constituent exposes the key, and its result is the union of the
//! constituents' property types. Only when some constituent lacks the key does
//! `tsc` fall back to the best-matching (discriminant-matched) member. The
//! derived target drives the property check, the leaf display, and the
//! nested-literal recursion alike — so a nested leaf reports against the
//! cross-arm union (`Type '2' is not assignable to type '1 | 9'.`), and a
//! property value that satisfies the cross-arm union produces no inner anchor
//! at all (the outer head with the folded property chain reports instead).

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// The `(check, display)` per-property target pair derived from the FULL
    /// (pre-narrowing) union target, mirroring `tsc`'s
    /// `getIndexedAccessTypeOrUndefined` step on a union.
    ///
    /// Returns `None` — the caller keeps its discriminant-narrowed derivation,
    /// which owns `tsc`'s best-matching-member fallback — when the target is
    /// not a multi-member union, when some constituent does not expose the
    /// key, or when the union-level derivation itself declines.
    pub(in crate::error_reporter::call_errors) fn full_union_object_literal_property_target(
        &mut self,
        pre_narrow_target: TypeId,
        prop_name_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<(TypeId, TypeId)> {
        let resolved = self.resolve_type_for_property_access(pre_narrow_target);
        let members: Vec<TypeId> = query_common::union_members(self.ctx.types, resolved)
            .or_else(|| {
                let evaluated = self.evaluate_type_with_env(resolved);
                query_common::union_members(self.ctx.types, evaluated)
            })?
            .as_ref()
            .to_vec();
        if members.len() < 2 {
            return None;
        }
        // Every constituent must expose the key through the same derivation
        // the per-member elaboration uses; one member lacking it means the
        // indexed access over the union is undefined in `tsc`, and the
        // best-matching member owns the target instead.
        for member in members {
            self.object_literal_target_property_type(member, prop_name_idx, prop_name)?;
        }
        self.object_literal_target_property_type(pre_narrow_target, prop_name_idx, prop_name)
    }
}
