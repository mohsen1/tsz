//! Literal-preserving alias rewrites shared by assignability diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn format_unfolded_ts2739_source_display(
        &self,
        unfolded: TypeId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_skip_application_display_alias_chase()
            .with_skip_application_alias_names();
        formatter.format(unfolded).into_owned()
    }

    pub(in crate::error_reporter) fn ts2739_alias_of_application_source_display_text(
        &self,
        source: TypeId,
    ) -> Option<String> {
        self.ts2739_alias_of_application_source_display(source)
            .map(|unfolded| self.format_unfolded_ts2739_source_display(unfolded))
    }

    pub(in crate::error_reporter) fn nonmissing_ts2739_alias_source_display_text(
        &self,
        source: TypeId,
    ) -> Option<String> {
        if self.source_is_generic_alias_application(source) {
            return None;
        }
        self.ts2739_alias_of_application_source_display_text(source)
    }

    fn source_is_generic_alias_application(&self, source: TypeId) -> bool {
        if self
            .ctx
            .definition_store
            .find_def_for_type(source)
            .and_then(|def_id| self.ctx.definition_store.get(def_id))
            .is_some_and(|def| {
                def.kind == tsz_solver::def::DefKind::TypeAlias && def.type_params.is_empty()
            })
        {
            return false;
        }
        if self
            .ctx
            .types
            .get_display_alias(source)
            .and_then(|alias| crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias))
            .and_then(|def_id| self.ctx.definition_store.get(def_id))
            .is_some_and(|def| {
                def.kind == tsz_solver::def::DefKind::TypeAlias && def.type_params.is_empty()
            })
        {
            return false;
        }
        let application = crate::query_boundaries::common::application_info(self.ctx.types, source)
            .or_else(|| {
                let alias = self.ctx.types.get_display_alias(source)?;
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            });
        let Some((base, _)) = application else {
            return false;
        };
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
        else {
            return false;
        };
        self.ctx.definition_store.get(def_id).is_some_and(|def| {
            def.kind == tsz_solver::def::DefKind::TypeAlias && !def.type_params.is_empty()
        })
    }

    pub(in crate::error_reporter) fn apply_ts2739_nonliteral(
        &mut self,
        source: TypeId,
        source_display: String,
    ) -> String {
        if crate::error_reporter::assignability::display_is_literal_value(&source_display) {
            return source_display;
        }
        self.nonmissing_ts2739_alias_source_display_text(source)
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
