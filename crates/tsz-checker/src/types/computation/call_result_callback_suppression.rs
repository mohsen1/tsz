//! Callback-body diagnostic cascade suppression for call result diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Suppress TS2769 (no overload matches) when the failure originates inside a
    /// callback argument's body. The callback's own diagnostics already explain the
    /// mismatch, so the outer overload error would be a redundant cascade.
    pub(super) fn should_suppress_no_overload_due_to_callback_body_errors(
        &self,
        args: &[NodeIndex],
    ) -> bool {
        const CALLBACK_BODY_DIAGNOSTIC_CODES: &[u32] = &[2322, 2339, 2345, 2347, 7006, 7019, 7031];

        args.iter().copied().any(|arg_idx| {
            self.is_callback_like_argument(arg_idx)
                && self
                    .callback_body_spans(arg_idx)
                    .iter()
                    .any(|(start, end)| {
                        self.ctx.diagnostics.iter().any(|diag| {
                            diag.start >= *start
                                && diag.start < *end
                                && CALLBACK_BODY_DIAGNOSTIC_CODES.contains(&diag.code)
                        })
                    })
        })
    }
}
