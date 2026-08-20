//! Shared "deferred constraint-relative source" elaboration walk, used by
//! both the property-mismatch drill leaf (`nested_application_property_mismatch.rs`)
//! and the plain (non-property) top-level mismatch renderers. Extracted from
//! `nested_application_property_mismatch.rs` to keep that module under the
//! file-size cap; move-only, no behavior change.
use crate::diagnostics::{
    Diagnostic, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages, format_message,
};
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

    /// Append only the constraint-walk elaboration STEPS beneath an
    /// already-rendered top-level head line, for a caller (the depth-0 plain
    /// `TypeMismatch` fallthrough in `render_type_mismatch`) that builds the
    /// head pair itself and only needs `tsc`'s per-step walk lines appended
    /// beneath it — unlike [`Self::push_deferred_constraint_walk`], this does
    /// NOT re-render the head pair as its own elaboration line. Empty (a
    /// no-op) when `source` has no further constraint to walk.
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
        // The head this walk hangs beneath is the diagnostic's MAIN message
        // (the caller built the head pair itself), not an elaboration line, so
        // its first child follows the shared header child-depth convention (see
        // [`super::first_child_depth`]): a depth-0 header's first child stays at
        // depth 0, a nested header's children go one level deeper. This differs
        // from [`Self::push_deferred_constraint_walk`], where the head pair IS
        // an elaboration line at `base_depth`, so its children start at
        // `base_depth + 1`. Seeding at `base_depth + 1` here too over-indented
        // the whole walk by one level for a plain top-level mismatch — tsc
        // renders the first walk step at 2 spaces (`x[k]: T[K]` head, then
        // `T[keyof T]` one level in), tsz rendered it at 4 (#17718 witnesses
        // 2/3, and the concrete-receiver `Wares3[K]` IntrinsicTypeMismatch
        // head; the byte-exact regression is #17797).
        let first_child_depth = super::first_child_depth(base_depth);
        for (i, step) in steps.iter().enumerate() {
            let depth = first_child_depth + i as u32;
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
        let line = self.constraint_walk_line_text(source, target, resolve_strip);
        diag.push_elaboration(
            line,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            depth,
        );
    }

    /// The `Type 'S' is not assignable to type 'T'.` text of one constraint-walk
    /// line, with the same nullish-strip policy as [`Self::push_constraint_walk_line`]:
    /// `target` renders in full for a union or deferred source and collapsed to
    /// its single real member for a concrete source. Extracted so both the
    /// `Diagnostic`-mutating push path and the pre-built related-info path
    /// ([`Self::argument_deferred_constraint_walk_related`]) render identical text.
    fn constraint_walk_line_text(
        &mut self,
        source: TypeId,
        target: TypeId,
        resolve_strip: bool,
    ) -> String {
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
        format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        )
    }

    /// The constraint-walk elaboration to layer beneath a TS2345 argument head
    /// whose source is a deferred, constraint-relative indexed access, or empty
    /// when the walk does not apply to this argument surface.
    ///
    /// `tsc` renders the argument source at its apparent type. When the base is
    /// still generic (`TE[KE]`, a bare `keyof T`, a conditional), the apparent
    /// type stays the as-written operand and `tsc` walks its constraint one step
    /// per line beneath the head — the head-kept path this fills in. When the
    /// base is CONCRETE (`Goods[KG]` over an interface), the apparent type is
    /// instead the resolved value type, which `tsc` collapses onto the head line
    /// itself (`boolean` vs `string`) with no as-written operand to walk beneath;
    /// that head-collapse is a distinct materialize-or-defer display concern
    /// (#15396 family) owned elsewhere, so this declines it rather than layering
    /// a walk beneath a (differently) wrong head. The two are told apart
    /// structurally: a concrete-base walk's first step is a concrete leaf, a
    /// generic-base walk's is not.
    pub(in crate::error_reporter) fn argument_deferred_constraint_walk_related(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        start: u32,
        length: u32,
    ) -> Vec<DiagnosticRelatedInformation> {
        if !self.is_deferred_constraint_relative_source(arg_type) {
            return Vec::new();
        }
        let steps = crate::query_boundaries::diagnostics::indexed_access_constraint_display_walk(
            self.ctx.types.as_type_database(),
            arg_type,
            param_type,
        );
        // A concrete-base access collapses to its value type on the head line
        // (owned by the materialize-or-defer display gateway), and a source with
        // no further constraint (a bare `keyof T`, whose base key space renders
        // through the `PropertyKey` alias) has no walk to emit — both leave the
        // argument head as its own owner renders it.
        if steps.first().is_none_or(|first| first.concrete) {
            return Vec::new();
        }
        // The head this walk hangs beneath is the argument diagnostic's MAIN
        // message, so its first child follows the shared header child-depth
        // convention ([`super::first_child_depth`]) — the same seeding as the
        // top-level TS2322 head's `push_deferred_constraint_walk_steps`, but
        // built as pre-built related-info lines because the argument head is
        // rendered through a `DiagnosticRenderRequest` and layers this on via
        // `extra_related` rather than mutating a `Diagnostic` directly. Every
        // line anchors on the head's own span (a chain link, not a
        // cross-location pointer).
        let first_child_depth = super::first_child_depth(0);
        let file = self.ctx.file_name.clone();
        steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                // Only a CONCRETE walk step collapses the nullable target to its
                // single real member; a still-deferred generic-base step keeps
                // the full union, so it must not be resolved for the strip
                // decision — the same rule as the `Diagnostic`-push path.
                let line = self.constraint_walk_line_text(step.type_id, param_type, step.concrete);
                // `related_message` seeds depth 0; `with_depth_shift` owns the
                // shift-and-clamp-into-`u8` so this does not re-spell it.
                Diagnostic::related_message(
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    file.clone(),
                    start,
                    length,
                    line,
                )
                .with_depth_shift(i64::from(first_child_depth + i as u32))
            })
            .collect()
    }
}
