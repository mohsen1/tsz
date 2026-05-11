//! Literal-preserving alias rewrites shared by assignability diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn apply_ts2739_nonliteral(
        &mut self,
        source: TypeId,
        source_display: String,
    ) -> String {
        if crate::error_reporter::assignability::display_is_literal_value(&source_display) {
            return source_display;
        }
        self.ts2739_alias_of_application_source_display_text(source)
            .unwrap_or(source_display)
    }

    pub(in crate::error_reporter) fn apply_eval_alias_nonliteral(
        &mut self,
        source: TypeId,
        source_display: String,
    ) -> String {
        if crate::error_reporter::assignability::display_is_literal_value(&source_display) {
            return source_display;
        }
        self.evaluated_literal_alias_source_display(source)
            .unwrap_or(source_display)
    }
}
