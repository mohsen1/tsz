//! Literal Type Utilities Module

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Validate regex literal flags against the compilation target.
    ///
    /// NOTE: TS1501 was removed in tsc 6.0 — the regex flag target check is no
    /// longer emitted. This method is retained as a no-op stub so that callers
    /// don't need to be updated.
    pub(crate) const fn validate_regex_literal_flags(
        &mut self,
        _idx: tsz_parser::parser::NodeIndex,
    ) {
    }
}
