//! Render the TS2322 + TS5075 index-access type-parameter mismatch elaboration.
//! Extracted from `render_failure.rs` to keep the module under the file-size cap.
use super::*;

impl<'a> CheckerState<'a> {
    /// Render the TS2322 + TS5075 elaboration chain for two distinct
    /// type-parameter keys of structurally-identical index accesses.
    ///
    /// tsc emits, for `S[T1] = c1 as S[T2]`:
    ///
    /// ```text
    /// error TS2322: Type 'S[T1]' is not assignable to type 'S[T2]'.
    ///   Type 'T1' is not assignable to type 'T2'.
    ///     'T1' is assignable to the constraint of type 'T2', but 'T2'
    ///     could be instantiated with a different subtype of constraint
    ///     '<constraint>'.
    /// ```
    ///
    /// The structural rule is independent of name choice: the elaboration
    /// uses whichever surface type parameters the user wrote, and falls
    /// back to a single-line message when the target parameter is
    /// unconstrained (no useful TS5075 anchor).
    pub(super) fn render_index_access_type_parameter_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_param: TypeId,
        target_param: TypeId,
        target_constraint: Option<TypeId>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(source, target, idx);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            file_name,
            start,
            length,
            message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        let source_param_str = self.format_type_diagnostic(source_param);
        let target_param_str = self.format_type_diagnostic(target_param);
        let share_declared_param_name =
            crate::query_boundaries::diagnostics::distinct_type_parameters_share_declared_name(
                self.ctx.types,
                source_param,
                target_param,
            );
        let (inner, inner_code) = if share_declared_param_name {
            (
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                    &[&source_param_str, &target_param_str],
                ),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
            )
        } else {
            (
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_param_str, &target_param_str],
                ),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };
        diag.push_elaboration(inner, inner_code, 0);
        if let Some(constraint) = target_constraint {
            let constraint_str = self.format_type_diagnostic(constraint);
            let elaboration = format_message(
                diagnostic_messages::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE,
                &[&source_param_str, &target_param_str, &constraint_str],
            );
            diag.push_elaboration(
                elaboration,
                diagnostic_codes::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE,
                0,
            );
        }
        diag
    }
}
