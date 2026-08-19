//! Union-target member-frame elaboration for structural best-member failures.
//!
//! When a non-fresh source fails against a union target and the solver's
//! best-member selection (`SubtypeChecker::select_union_target_best_member`)
//! picks a constituent whose failure is *not* a missing required property,
//! `tsc` (`getBestMatchingType`) re-runs the failed relation against that
//! member with errors enabled — so the diagnostic chain continues past the
//! union head with a member frame and the member relation's own drill. The
//! missing-property fold and the fresh-object-literal expression elaboration
//! keep their existing paths; this renderer owns only the member-frame shape.

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::render_failure::RenderContext;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Render a union-target failure whose best-matching member fails through
    /// a structural (non-missing-property) reason: the union headline, then
    /// the member frame `Type 'S' is not assignable to type '<member>'.`, then
    /// the member relation's own drill one level deeper.
    ///
    /// `tsc` (`getBestMatchingType`) re-runs the failed relation against the
    /// selected member with errors enabled, so the member frame heads the
    /// member's structural elaboration (`Types of property 'm' are
    /// incompatible.` → the leaf, or the path-compressed `The types of 'm.p'
    /// are incompatible between these types.` form). A missing-property
    /// failure instead folds beneath the headline with no frame (see the
    /// `UnionTargetMismatch` dispatch arm); a plain leaf collapses to the
    /// member frame itself.
    pub(super) fn render_union_target_member_frame_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_type: TypeId,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        // Top-level union headline (`Type 'S' is not assignable to type
        // 'A | B'.`) — identical to the head the bare union line rendered
        // before elaboration existed, and to the missing-property fold's head.
        let mut diag = if depth == 0 {
            self.render_type_mismatch(ctx)
        } else {
            let source_str = self.format_type_diagnostic(source_type);
            let target_str = self.format_type_diagnostic(target_type);
            Diagnostic::error(
                file_name.clone(),
                start,
                length,
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                ),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };

        if depth >= 5 {
            return diag;
        }
        // The member relation sits one indent level beneath the headline. At
        // depth 0 the headline is the (un-indented) primary, so its first
        // child is related-depth 0; nested, the headline is at related-depth
        // `depth`, so the child is at `depth + 1`.
        let child_depth = if depth == 0 { 0 } else { depth + 1 };

        // Member frame `Type 'S' is not assignable to type '<member>'.`
        // (structural display, so the member — not the whole union — is
        // named). The frame is a nested relation line, so a literal source
        // generalizes against the member (tsc `reportRelationError`).
        let display_source =
            self.generalize_nested_relation_source_for_display(source_type, member_type);
        let frame_source = self.format_type_diagnostic(display_source);
        let frame_target = self.format_type_diagnostic(member_type);
        diag.push_elaboration_at(
            file_name,
            start,
            length,
            format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&frame_source, &frame_target],
            ),
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            child_depth,
        );

        // Render the `S <: member` relation as a standalone diagnostic
        // (depth 0) so its drill keeps `tsc`'s path-compressed shape (`The
        // types of 'x.p' are incompatible …`, which the property renderer only
        // produces at depth 0), then drop its anchor-derived headline (already
        // expressed by the frame above) and slot the remaining drill one level
        // beneath the frame. A plain leaf carries no drill, so the frame
        // stands alone.
        let sub = self.render_failure_reason(nested_reason, source_type, member_type, idx, 0);
        let drill_base = i64::from(child_depth + 1);
        for related in sub.related_information {
            diag.related_information
                .push(related.with_depth_shift(drill_base));
        }

        diag
    }
}
