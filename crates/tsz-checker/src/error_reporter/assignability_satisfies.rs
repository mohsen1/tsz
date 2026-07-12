//! TS1360 (`satisfies`) assignability error reporting.
//!
//! Split out of `assignability.rs` to keep that module under the 2000-line
//! architecture ceiling (§19). A `satisfies` failure reuses the same
//! assignability elaboration a plain assignment would build; only the head
//! message differs. See `error_type_does_not_satisfy_the_expected_type` for the
//! replace-vs-wrap rule that mirrors tsc's `checkSatisfiesExpression`.

use crate::error_reporter::fingerprint_policy::{DiagnosticAnchorKind, RelatedInformationPolicy};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub fn error_type_does_not_satisfy_the_expected_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) {
        if !self.has_exact_optional_property_mismatch(source, target)
            && self.should_suppress_assignability_diagnostic(source, target)
        {
            return;
        }

        let reason = self
            .analyze_assignability_failure(source, target)
            .failure_reason;

        // For TS1360, point the diagnostic at the `satisfies` keyword position
        // when available, rather than walking up to the enclosing statement.
        let anchor_idx = if keyword_pos.is_some() {
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::Exact)
        } else {
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment)
        };

        let mut base_diag = match reason {
            Some(reason) => self.render_failure_reason(&reason, source, target, anchor_idx, 0),
            None => {
                let Some(anchor) =
                    self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                else {
                    return;
                };
                let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                    self.ctx.file_name.as_str(),
                )
                .with_def_store(&self.ctx.definition_store)
                .with_namespace_module_names(&self.ctx.namespace_module_names);
                let diag = builder.type_not_assignable(source, target, anchor.start, anchor.length);
                diag.to_checker_diagnostic(&self.ctx.file_name)
            }
        };

        // Mutate the top-level diagnostic to be TS1360.
        // When the target is not literal-sensitive (e.g. `1 satisfies boolean`),
        // widen a bare literal source for display to match tsc, which reports
        // `Type 'number' does not satisfy the expected type 'boolean'.`
        // (tsc's `typeToString` widens fresh literal primitives when the target
        // type does not preserve literal display.)
        let display_source = if self.is_literal_sensitive_assignment_target(target) {
            source
        } else {
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, source)
        };
        let src_str = self.format_type_for_assignability_message(display_source);
        let tgt_str = self.format_type_for_assignability_message(target);
        use tsz_common::diagnostics::data::diagnostic_codes;
        use tsz_common::diagnostics::data::diagnostic_messages;
        use tsz_common::diagnostics::format_message;

        let msg = format_message(
            diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE,
            &[&src_str, &tgt_str],
        );

        // tsc 7.0.2 renders a `satisfies` failure through the *same*
        // assignability elaboration a plain assignment would build, and the
        // property-missing family stays the TOP-LEVEL diagnostic — verified:
        // `{} satisfies I1` gets TS2739/TS2741 at the `satisfies` keyword with
        // no TS1360 anywhere. Only the generic relation head is replaced by
        // the `does not satisfy` wording (e.g. a primitive source keeps
        // TS1360). Other specific codes (inner property TS2322, TS2353) never
        // reach this reporter; they surface from the elaboration path.
        let is_missing_property_head = matches!(
            base_diag.code,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        ) && self
            .missing_property_head_promotion_applies(source, target);
        if base_diag.code != diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE
            && !is_missing_property_head
        {
            let top_is_generic_relation =
                base_diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
            if !top_is_generic_relation {
                base_diag.related_information = self.related_from_diagnostic(
                    &base_diag,
                    RelatedInformationPolicy::WRAPPED_DIAGNOSTIC,
                );
            }
            base_diag.code = diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE;
            base_diag.message_text = msg;
        }

        // Override the diagnostic start position to the `satisfies` keyword
        // when available. tsc points TS1360 at the keyword, not the expression.
        if let Some(kw_pos) = keyword_pos {
            base_diag.start = kw_pos;
            // "satisfies" is 9 characters long
            base_diag.length = 9;
        }

        self.ctx.push_diagnostic(base_diag);
    }
}
