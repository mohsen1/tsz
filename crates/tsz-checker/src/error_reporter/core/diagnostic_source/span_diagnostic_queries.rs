use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn has_more_specific_diagnostic_at_span(
        &self,
        start: u32,
        length: u32,
    ) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.code
                    != diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV
        })
    }

    pub(crate) fn has_diagnostic_code_within_span(&self, start: u32, end: u32, code: u32) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start >= start && diag.start < end)
    }
}

/// Strip TS-family file extensions from module specifiers for display while
/// preserving JS-family extensions in `typeof import("mod")` output.
/// Element-access diagnostics can opt into raw namespace display earlier.
pub(crate) fn strip_module_specifier_extension(module_name: &str) -> &str {
    tsz_common::file_extensions::strip_ts_extension(module_name)
}
