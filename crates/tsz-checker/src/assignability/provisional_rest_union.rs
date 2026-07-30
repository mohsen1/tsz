//! Generic-call aggregate variadic-rest assignability.
//!
//! This lives beside the ordinary assignability relation implementation so the
//! checker-facing file stays below the repository line limit while the typed
//! solver policy remains behind the canonical query boundary.

use crate::query_boundaries::assignability::AssignabilityQueryInputs;
use crate::state::{CheckerOverrideProvider, CheckerState};
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Strict generic-call aggregate-rest validation. Inferred variadic unions
    /// remain provisional while the call evaluator separately validates their
    /// concrete fixed prefix and suffix arguments.
    pub(crate) fn is_assignable_to_provisional_rest_union(
        &mut self,
        source: TypeId,
        target: TypeId,
        overload_subtype_pass: bool,
    ) -> bool {
        self.generic_call_strict_relation(source, target, overload_subtype_pass, true, true, false)
    }

    /// Generic-call strict relation that preserves the declaration surface.
    ///
    /// The solver owns rest-binder semantics; this boundary merely avoids the
    /// checker's ordinary eager preparation, which would replace an outer
    /// `...T` with its array constraint before the solver sees it.
    pub(crate) fn is_assignable_to_generic_call_raw(
        &mut self,
        source: TypeId,
        target: TypeId,
        overload_subtype_pass: bool,
        strict: bool,
        provisional_rest_union: bool,
    ) -> bool {
        self.generic_call_strict_relation(
            source,
            target,
            overload_subtype_pass,
            strict,
            provisional_rest_union,
            true,
        )
    }

    fn generic_call_strict_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
        overload_subtype_pass: bool,
        strict: bool,
        provisional_rest_union: bool,
        preserve_raw_surface: bool,
    ) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = if preserve_raw_surface {
            self.ensure_relation_inputs_ready(&[source, target]);
            (
                self.substitute_this_type_if_needed(source),
                self.substitute_this_type_if_needed(target),
            )
        } else {
            self.prepare_assignability_inputs(source, target)
        };
        let mut flags = self.ctx.pack_relation_flags();
        if strict {
            flags |= crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES;
        }
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source,
            target,
            flags,
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
            evaluation_session: Some(self.ctx.eval_session.as_ref()),
        };
        let relation_result = match (overload_subtype_pass, provisional_rest_union) {
            (true, true) => {
                crate::query_boundaries::assignability::cached_overload_subtype_pass_provisional_rest_union_assignability(
                    &inputs,
                    &overrides,
                )
            }
            (true, false) => {
                crate::query_boundaries::assignability::cached_overload_subtype_pass_assignability(
                    &inputs,
                    &overrides,
                )
            }
            (false, true) => {
                crate::query_boundaries::assignability::cached_provisional_rest_union_assignability(
                    &inputs, &overrides,
                )
            }
            (false, false) => {
                crate::query_boundaries::assignability::cached_assignability_with_overrides(
                    &inputs, &overrides,
                )
            }
        };
        let result = relation_result.is_related();

        self.propagate_overflow_flags(
            relation_result.depth_exceeded(),
            relation_result.iteration_exceeded(),
        );

        result
    }
}
