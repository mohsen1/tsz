//! Deferred, constraint-relative source display: the constraint-walk
//! elaboration `tsc` renders beneath a `T[K]`/`keyof T`/conditional operand
//! that keeps its as-written spelling on the head or leaf pair line.
//!
//! Extracted from `nested_application_property_mismatch.rs` for the checker
//! arch-size ceiling. Two call shapes share the same underlying solver walk
//! ([`indexed_access_constraint_display_walk`]): the property-mismatch drill
//! leaf, which has no head of its own and needs the walk's first line to
//! serve as its leaf pair ([`CheckerState::push_deferred_constraint_walk`]);
//! and a top-level TS2322 head, which already rendered the as-written
//! operand as the diagnostic's own message and only owes the steps beneath
//! it ([`CheckerState::push_deferred_constraint_walk_steps`]).
//!
//! [`indexed_access_constraint_display_walk`]: crate::query_boundaries::diagnostics::indexed_access_constraint_display_walk

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Emit the deferred-constraint-relative walk for a property leaf when
    /// `source` is such an operand (`T[K]`, `keyof T`, a conditional, or a
    /// generic alias application still deferred through its arguments), returning
    /// whether it fired. Every property-drill leaf that keeps the as-written
    /// operand — the single-property drill, the dotted-path collapse, and the
    /// call-argument surfaces — funnels its source through this one predicate and
    /// [`Self::push_deferred_constraint_walk`], so the "deferred source -> walk"
    /// decision (and the operand classifier it depends on) lives in a single
    /// place rather than being restated at each renderer.
    pub(super) fn try_push_deferred_constraint_walk(
        &mut self,
        diag: &mut Diagnostic,
        source: TypeId,
        target: TypeId,
        base_depth: u32,
    ) -> bool {
        if self.is_deferred_constraint_relative_source(source) {
            self.push_deferred_constraint_walk(diag, source, target, base_depth);
            true
        } else {
            false
        }
    }

    /// Emit a deferred, constraint-relative source's leaf pair and the
    /// constraint-walk elaboration `tsc` renders beneath it.
    ///
    /// `tsc` keeps the as-written operand and the full nullable-union target on
    /// the first line (`TBox[KKey]` vs `string | undefined`), then walks the
    /// operand's constraint one step per line
    /// ([`indexed_access_constraint_display_walk`]), collapsing the target to
    /// its single real member only once a walk step reaches a concrete source
    /// (`number` vs `string`) — a deferred or union step keeps the full union.
    /// `base_depth` is the elaboration depth of the leaf pair line.
    ///
    /// [`indexed_access_constraint_display_walk`]: crate::query_boundaries::diagnostics::indexed_access_constraint_display_walk
    pub(super) fn push_deferred_constraint_walk(
        &mut self,
        diag: &mut Diagnostic,
        source: TypeId,
        target: TypeId,
        base_depth: u32,
    ) {
        // The as-written operand renders verbatim with the full nullable union
        // (its deferred form defers its relation to the constraint), so its
        // strip decision must NOT resolve it — `Obj[KP]` would otherwise reduce
        // to its constraint `number` and collapse the union prematurely.
        self.push_constraint_walk_line(diag, source, target, base_depth, false);
        let steps = crate::query_boundaries::diagnostics::indexed_access_constraint_display_walk(
            self.ctx.types.as_type_database(),
            source,
            target,
        );
        for (i, step) in steps.iter().enumerate() {
            let depth = base_depth + 1 + i as u32;
            // Only a CONCRETE walk step (its object resolved from a real type)
            // collapses the target to its single real member; a still-deferred
            // generic-base step keeps the full nullable union, so it must not be
            // resolved for the strip decision.
            self.push_constraint_walk_line(diag, step.type_id, target, depth, step.concrete);
        }
    }

    /// Emit only the constraint-walk elaboration lines beneath a deferred,
    /// constraint-relative source, without the leading as-written-operand
    /// line [`Self::push_deferred_constraint_walk`] also emits.
    ///
    /// The top-level TS2322 head (`render_type_mismatch`, plus the sibling
    /// `NoUnionMemberMatches`/catch-all sites in `render_failure.rs`) already
    /// renders the as-written operand as the diagnostic's own message —
    /// unlike the property-mismatch drill leaf, which has no head of its own
    /// and needs the walk's first line to serve as its leaf pair. Reusing
    /// [`Self::push_deferred_constraint_walk`] there would duplicate the head
    /// as an elaboration line.
    pub(super) fn push_deferred_constraint_walk_steps(
        &mut self,
        diag: &mut Diagnostic,
        source: TypeId,
        target: TypeId,
        base_depth: u32,
    ) {
        let steps = crate::query_boundaries::diagnostics::indexed_access_constraint_display_walk(
            self.ctx.types.as_type_database(),
            source,
            target,
        );
        for (i, step) in steps.iter().enumerate() {
            let depth = base_depth + i as u32;
            self.push_constraint_walk_line(diag, step.type_id, target, depth, step.concrete);
        }
    }

    /// Push one `Type 'S' is not assignable to type 'T'.` line for a constraint
    /// walk, rendering `target` in full for a union or deferred source and
    /// collapsed to its single real member for a concrete source (mirroring
    /// `tsc`'s `getBestMatchingType` at the concrete leaf only). `resolve_strip`
    /// resolves the source through the checker's resolver before the strip
    /// decision — set for a concrete walk step (where `Obj[keyof Obj]` reduces to
    /// `number`) but not for a still-deferred step or the verbatim as-written
    /// operand. This resolution is the checker's job by contract, not a patch:
    /// the solver flags the step `concrete`, and `DefId -> TypeId` resolution of
    /// a `Lazy` object base is owned by the checker's `TypeEnvironment`, so the
    /// solver deliberately leaves the final reduction to this call.
    fn push_constraint_walk_line(
        &mut self,
        diag: &mut Diagnostic,
        source: TypeId,
        target: TypeId,
        depth: u32,
        resolve_strip: bool,
    ) {
        // The nullish-strip decision must see the source the reader sees: a
        // resolved concrete-base access (`Obj[keyof Obj]` -> `number`) collapses
        // the target to its single real member; a still-deferred access or a
        // union keeps the full nullable union.
        let strip_source = if resolve_strip {
            self.evaluate_type_for_assignability(source)
        } else {
            source
        };
        let display_target =
            if crate::query_boundaries::common::union_members(self.ctx.types, strip_source)
                .is_some()
            {
                target
            } else {
                self.strip_nullish_for_assignability_display(target, strip_source)
                    .unwrap_or(target)
            };
        let source_str = self.format_type_for_assignability_message(source);
        let target_str = self.format_type_for_assignability_message(display_target);
        let line = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        diag.push_elaboration(
            line,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            depth,
        );
    }
}
