use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn format_type_diagnostic_for_assignability_display(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(in crate::error_reporter) fn format_type_diagnostic_widened_for_assignability_display(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(crate) fn format_type_for_property_receiver_message(&mut self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_application_alias_names()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(crate) fn truncate_property_receiver_display(display: String) -> String {
        const MAX_PROPERTY_RECEIVER_DISPLAY_CHARS: usize = 320;
        let should_truncate = display.starts_with("Omit<") || display.starts_with("merge<");
        if display.len() <= MAX_PROPERTY_RECEIVER_DISPLAY_CHARS || !should_truncate {
            return display;
        }
        let display =
            super::super::property_receiver_formatting::elide_long_property_receiver_object_literals(
                display,
            );
        if display.starts_with("merge<") {
            let mut truncated: String = display
                .chars()
                .take(MAX_PROPERTY_RECEIVER_DISPLAY_CHARS - 2)
                .collect();
            truncated.push_str("..");
            return truncated;
        }
        display
            .chars()
            .take(MAX_PROPERTY_RECEIVER_DISPLAY_CHARS)
            .collect()
    }

    pub(crate) fn format_long_property_receiver_type_for_diagnostic(&self, ty: TypeId) -> String {
        tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
            .with_def_store(&self.ctx.definition_store)
            .with_diagnostic_mode()
            .with_long_property_receiver_display()
            .with_skip_application_alias_names()
            .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
            .format(ty)
            .into_owned()
    }
}
