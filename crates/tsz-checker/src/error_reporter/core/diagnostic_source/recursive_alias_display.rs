use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn recursive_alias_application_source_display(
        &mut self,
        expr_idx: NodeIndex,
        declared_type: TypeId,
    ) -> Option<String> {
        if !crate::query_boundaries::recursive_alias::is_recursive_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            declared_type,
        ) {
            return None;
        }
        let annotation = self.declared_diagnostic_source_annotation_text(expr_idx)?;
        Some(self.format_annotation_like_type(&annotation))
    }
}
