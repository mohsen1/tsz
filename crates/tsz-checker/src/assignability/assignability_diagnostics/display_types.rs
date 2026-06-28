use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Like `check_assignable_or_report_at_without_source_elaboration`, but allows
    /// specifying separate types for display purposes. This is used when checking
    /// assignability of return types but displaying the full function types in error
    /// messages (e.g., "Type '() => string' is not assignable to type
    /// '{ (): number; (i: number): number; }'").
    pub(crate) fn check_assignable_or_report_at_with_display_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_for_display: TypeId,
        target_for_display: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at_with_display_types_and_options(
            source,
            target,
            source_for_display,
            target_for_display,
            source_idx,
            diag_idx,
            false,
            false,
        )
    }

    /// Like `check_assignable_or_report_at_with_display_types`, but renders the
    /// target VERBATIM (alias name and `| null | undefined` preserved) instead of
    /// through the nullish-stripping `AssignmentTarget` role. Used for JSX
    /// framework special attributes (`key`/`ref`), which `tsc` elaborates with
    /// their full declared apparent type. `check_target` drives the relation;
    /// `display_target` is shown in the message.
    pub(crate) fn check_assignable_or_report_at_with_verbatim_target_display(
        &mut self,
        source: TypeId,
        check_target: TypeId,
        display_target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at_with_display_types_and_options(
            source,
            check_target,
            source,
            display_target,
            source_idx,
            diag_idx,
            false,
            true,
        )
    }

    /// Like `check_assignable_or_report_at_with_display_types`, but keeps the
    /// diagnostic anchored at `diag_idx` without drilling into the source shape.
    pub(crate) fn check_assignable_or_report_at_exact_anchor_without_source_elaboration_with_display_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_for_display: TypeId,
        target_for_display: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at_with_display_types_and_options(
            source,
            target,
            source_for_display,
            target_for_display,
            source_idx,
            diag_idx,
            true,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_assignable_or_report_at_with_display_types_and_options(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_for_display: TypeId,
        target_for_display: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
        skip_source_elaboration: bool,
        verbatim_target: bool,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }

        // Check assignability using the actual types (return types)
        if self
            .assignability_reason_relation_outcome(source, target)
            .related
        {
            return true;
        }

        // Get the failure reason using the check types
        let analysis = self.analyze_assignability_failure(source, target);

        // Try to elaborate the source error first
        if !skip_source_elaboration
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }

        // Framework special-attribute (`key`/`ref`) targets are rendered verbatim
        // so the declared alias and `| null | undefined` survive. `tsc` shows the
        // plain "Type X is not assignable to type Y" form here (no nested reason),
        // so skip the reason-based elaboration entirely.
        if verbatim_target {
            self.error_type_not_assignable_at_with_verbatim_target(
                source_for_display,
                target_for_display,
                diag_idx,
            );
            return false;
        }

        // Report the error using the display types (full function types)
        if let Some(ref reason) = analysis.failure_reason {
            // For simple type mismatches, use the error reporter method to render
            // with display types.
            if matches!(
                reason,
                tsz_solver::SubtypeFailureReason::TypeMismatch { .. }
                    | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                    | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }
            ) {
                self.error_type_not_assignable_at_with_display_types(
                    source_for_display,
                    target_for_display,
                    diag_idx,
                );
            } else {
                self.error_type_not_assignable_with_reason_and_display(
                    source_for_display,
                    target_for_display,
                    reason,
                    diag_idx,
                );
            }
        } else {
            self.error_type_not_assignable_with_reason_at(
                source_for_display,
                target_for_display,
                diag_idx,
            );
        }
        false
    }
}
