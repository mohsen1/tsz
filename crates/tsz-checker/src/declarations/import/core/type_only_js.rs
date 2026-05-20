use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn should_report_js_type_only_import_diagnostic(
        &self,
        clause_is_type_only: bool,
        specifier_is_type_only: bool,
    ) -> bool {
        self.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && !clause_is_type_only
            && !specifier_is_type_only
    }

    pub(super) fn emit_js_type_only_import_diagnostic(
        &mut self,
        report_at: NodeIndex,
        import_name: &str,
        module_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let clean_module = module_name.trim_matches('\'').trim_matches('"');
        let quoted_import = format!("import(\"{clean_module}\").{import_name}");
        let message = format_message(
            diagnostic_messages::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT,
            &[import_name, &quoted_import],
        );
        let start = self.ctx.arena.get(report_at).map_or(0, |n| n.pos);
        if self.ctx.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT
                && diag.start == start
        }) {
            return;
        }
        self.error_at_node(
            report_at,
            &message,
            diagnostic_codes::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT,
        );
    }
}
