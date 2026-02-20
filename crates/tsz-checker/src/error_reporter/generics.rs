//! Generic type and comparison error reporting (TS2314, TS2344, TS2367, TS2352).

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Generic Type Errors
    // =========================================================================

    /// Report TS2314: Generic type 'X' requires N type argument(s).
    pub fn error_generic_type_requires_type_arguments_at(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                &[name, &required_count.to_string()],
            );
            // Use push_diagnostic for deduplication - same type may be resolved multiple times
            self.ctx.push_diagnostic(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            ));
        }
    }

    /// Report TS2344: Type does not satisfy constraint.
    pub fn error_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if type_arg == TypeId::ERROR
            || constraint == TypeId::ERROR
            || type_arg == TypeId::UNKNOWN
            || constraint == TypeId::UNKNOWN
            || type_arg == TypeId::ANY
            || constraint == TypeId::ANY
        {
            return;
        }

        // Also suppress when either side CONTAINS error types (e.g., { new(): error }).
        // This happens when a forward-referenced class hasn't been fully resolved yet.
        if tsz_solver::type_queries::contains_error_type_db(self.ctx.types, type_arg)
            || tsz_solver::type_queries::contains_error_type_db(self.ctx.types, constraint)
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Deduplicate: get_type_from_type_node may re-resolve type references when
            // type_parameter_scope changes, causing validate_type_reference_type_arguments
            // to be called multiple times for the same node.
            let key = (
                loc.start,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            );
            if self.ctx.emitted_diagnostics.contains(&key) {
                return;
            }
            self.ctx.emitted_diagnostics.insert(key);

            let type_str = self.format_type(type_arg);
            let constraint_str = self.format_type(constraint);
            let message = format_message(
                diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[&type_str, &constraint_str],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            ));
        }
    }

    /// Report TS2367: This condition will always return 'false'/'true' since the types have no overlap.
    ///
    /// The message depends on the operator:
    /// - For `===` and `==`: "always return 'false'"
    /// - For `!==` and `!=`: "always return 'true'"
    pub fn error_comparison_no_overlap(
        &mut self,
        left_type: TypeId,
        right_type: TypeId,
        is_equality: bool,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::ANY
            || right_type == TypeId::ANY
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let left_str = self.format_type(left_type);
            let right_str = self.format_type(right_type);
            let result = if is_equality { "false" } else { "true" };
            let message = format_message(
                diagnostic_messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA,
                &[result, &left_str, &right_str],
            );
            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), message, diagnostic_codes::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA));
        }
    }

    /// Report TS2352: Conversion of type 'X' to type 'Y' may be a mistake because neither type
    /// sufficiently overlaps with the other. If this was intentional, convert the expression to 'unknown' first.
    pub fn error_type_assertion_no_overlap(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let source_str = self.format_type(source_type);
            let target_str = self.format_type(target_type);
            let message = format_message(
                diagnostic_messages::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
                &[&source_str, &target_str],
            );
            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), message, diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV));
        }
    }

    // =========================================================================
    // Diagnostic Utilities
    // =========================================================================

    /// Create a diagnostic collector for batch error reporting.
    pub fn create_diagnostic_collector(&self) -> tsz_solver::DiagnosticCollector<'_> {
        tsz_solver::DiagnosticCollector::new(self.ctx.types, self.ctx.file_name.as_str())
    }

    /// Merge diagnostics from a collector into the checker's diagnostics.
    pub fn merge_diagnostics(&mut self, collector: &tsz_solver::DiagnosticCollector) {
        for diag in collector.to_checker_diagnostics() {
            self.ctx.diagnostics.push(diag);
        }
    }
}
