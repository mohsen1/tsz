//! Overload-resolution subtype-pass relation entries (tsc `chooseOverload`
//! with `subtypeRelation`; issue #13042).
//!
//! Pass 1 of overload resolution runs assignability under the solver's
//! `AnySourceNotRelated` propagation mode: an `any` source is not related to
//! non-`any`/`unknown` targets at every nesting level, while an `any` target
//! still accepts everything. The mode is part of the relation cache key, so
//! pass-1 results cannot poison the default assignable relation.

use crate::query_boundaries::assignability::AssignabilityQueryInputs;
use crate::state::{CheckerOverrideProvider, CheckerState};
use tracing::trace;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Shared core for the overload-resolution subtype pass (tsc
    /// `chooseOverload` with `subtypeRelation`): assignability where an `any`
    /// source is not related to non-`any`/`unknown` targets at every nesting
    /// level, while an `any` target still accepts everything. The mode is
    /// part of the relation cache key, so pass-1 results cannot poison the
    /// default assignable relation (or vice versa).
    fn check_overload_subtype_pass_assignability(
        &mut self,
        source: TypeId,
        target: TypeId,
        extra_flags: u16,
        label: &str,
    ) -> bool {
        let flags = self.ctx.pack_relation_flags() | extra_flags;
        let overrides = CheckerOverrideProvider::new(self, None);
        let relation_result =
            crate::query_boundaries::assignability::cached_overload_subtype_pass_assignability(
                &AssignabilityQueryInputs {
                    db: self.ctx.types,
                    resolver: &self.ctx,
                    source,
                    target,
                    flags,
                    inheritance_graph: &self.ctx.inheritance_graph,
                    sound_mode: self.ctx.sound_mode(),
                },
                &overrides,
            );
        let result = relation_result.is_related();

        self.propagate_overflow_flags(
            relation_result.depth_exceeded,
            relation_result.iteration_exceeded,
        );

        trace!(source = source.0, target = target.0, result, "{label}");
        result
    }

    /// Like `is_assignable_to_strict`, but under the overload-resolution
    /// subtype pass where an `any` source is not related to concrete targets.
    pub fn is_assignable_to_overload_subtype_pass(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_overload_subtype_pass_assignability(
            source,
            target,
            0,
            "is_assignable_to_overload_subtype_pass",
        )
    }

    /// Strict-function-types variant of
    /// [`Self::is_assignable_to_overload_subtype_pass`].
    pub fn is_assignable_to_overload_subtype_pass_strict(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_overload_subtype_pass_assignability(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES,
            "is_assignable_to_overload_subtype_pass_strict",
        )
    }
}
