//! Argument-mismatch reporting for `new`-expression calls.
//!
//! Owns the diagnostic-shape decision for a `CallResult::ArgumentTypeMismatch`
//! on a `new` expression, mirroring the call-expression result handler:
//! object/array literal arguments elaborate per-property `TS2322` errors,
//! while a context-sensitive callback with a block body reports a single
//! argument-level `TS2345` (tsc's elaboration only drills concise bodies).

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report a failed `new`-expression argument at `args[index]`.
    ///
    /// `new_target_expr` is the constructor callee expression (used only for
    /// the weak-union/excess-property suppression probe).
    pub(crate) fn report_new_expression_argument_mismatch(
        &mut self,
        new_target_expr: NodeIndex,
        args: &[NodeIndex],
        index: usize,
        actual: TypeId,
        expected: TypeId,
        arg_idx: NodeIndex,
    ) {
        // Weak union violation / excess property case: TypeScript shows
        // TS2353 (excess property) instead, so skip the assignability report
        // regardless of the excess-property checking flag.
        if self.should_suppress_weak_key_arg_mismatch(new_target_expr, args, index, actual) {
            return;
        }
        // When a callback argument has a block body, tsc reports TS2345 at
        // the argument level rather than elaborating an inner TS2322 on its
        // return statements — same rule as the call-expression result handler.
        let prefer_argument_level_return_mismatch =
            self.callback_prefers_argument_level_return_mismatch(arg_idx);
        // Try to elaborate object/array literal arguments into
        // per-property/element TS2322 errors before falling back to a blanket
        // TS2345 on the whole argument.
        let elaborated = if !prefer_argument_level_return_mismatch
            && self.argument_supports_literal_elaboration(arg_idx)
        {
            self.try_elaborate_object_literal_arg_error(arg_idx, expected)
        } else {
            false
        };
        if prefer_argument_level_return_mismatch {
            // Scrub any TS2322 the argument-collection pass left inside the
            // callback span so the argument-level TS2345 is the only
            // diagnostic at the argument site.
            let body_spans = self.callback_body_spans(arg_idx);
            let arg_span = self.callback_argument_span(arg_idx);
            self.ctx.diagnostics.retain(|d| {
                !(d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    && (body_spans
                        .iter()
                        .any(|(start, end)| d.start >= *start && d.start < *end)
                        || arg_span.is_some_and(|(start, end)| d.start >= start && d.start < end)))
            });
            self.ctx.rebuild_emitted_diagnostics_from_current();
            self.error_argument_not_assignable_at(actual, expected, arg_idx);
        } else if !elaborated {
            let _ = self.check_argument_assignable_or_report(actual, expected, arg_idx);
        }
    }
}
