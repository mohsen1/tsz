//! Generic type and comparison error reporting (TS2314, TS2344, TS2367, TS2352).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn widen_function_like_assertion_source(&self, type_id: TypeId) -> TypeId {
        if let Some(return_type) = get_function_return_type(self.ctx.types, type_id) {
            let widened_return = tsz_solver::widen_literal_type(self.ctx.types, return_type);
            if widened_return != return_type {
                let replaced =
                    replace_function_return_type(self.ctx.types, type_id, widened_return);
                if replaced != type_id {
                    return replaced;
                }
            }
        }

        if let Some(shape_id) = tsz_solver::callable_shape_id(self.ctx.types, type_id) {
            let shape = self.ctx.types.callable_shape(shape_id);
            let mut changed = false;

            let call_signatures = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let widened_return =
                        tsz_solver::widen_literal_type(self.ctx.types, sig.return_type);
                    if widened_return != sig.return_type {
                        changed = true;
                        let mut next = sig.clone();
                        next.return_type = widened_return;
                        next
                    } else {
                        sig.clone()
                    }
                })
                .collect();

            let construct_signatures = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let widened_return =
                        tsz_solver::widen_literal_type(self.ctx.types, sig.return_type);
                    if widened_return != sig.return_type {
                        changed = true;
                        let mut next = sig.clone();
                        next.return_type = widened_return;
                        next
                    } else {
                        sig.clone()
                    }
                })
                .collect();

            if changed {
                return self.ctx.types.callable(tsz_solver::CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties: shape.properties.clone(),
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                });
            }
        }

        type_id
    }

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
        let count_str = required_count.to_string();
        self.error_at_node_msg(
            idx,
            diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            &[name, &count_str],
        );
    }

    /// Report TS2314 at an explicit source location.
    pub fn error_generic_type_requires_type_arguments_at_span(
        &mut self,
        name: &str,
        required_count: usize,
        start: u32,
        length: u32,
    ) {
        let message = format_message(
            diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            &[name, &required_count.to_string()],
        );
        self.ctx.error(
            start,
            length,
            message,
            diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
        );
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
        if common::contains_error_type(self.ctx.types, type_arg)
            || common::contains_error_type(self.ctx.types, constraint)
        {
            return;
        }

        // tsc widens literal types to their base types in TS2344 messages:
        // e.g., `42` → `number`, `"hello"` → `string`. This matches
        // tsc's getBaseTypeOfLiteralType applied before typeToString.
        let widened_arg = tsz_solver::widen_literal_type(self.ctx.types, type_arg);
        let type_str = self.format_type_diagnostic(widened_arg);
        let constraint_str = self.format_type_diagnostic(constraint);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            &[&type_str, &constraint_str],
        );
    }

    /// Report TS2559: Type has no properties in common with constraint.
    ///
    /// Emitted instead of TS2344 when the constraint is a "weak type" (all-optional
    /// properties) and the type argument shares no common properties with it. tsc
    /// emits TS2559 in this case because the failure is specifically about weak type
    /// detection, not a general constraint violation.
    pub fn error_no_common_properties_constraint(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        if type_arg == TypeId::ERROR
            || constraint == TypeId::ERROR
            || type_arg == TypeId::ANY
            || constraint == TypeId::ANY
        {
            return;
        }

        let type_str = self.format_type_diagnostic(type_arg);
        let constraint_str = self.format_type_diagnostic(constraint);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
            &[&type_str, &constraint_str],
        );
    }

    /// Report TS2352: Conversion of type 'X' to type 'Y' may be a mistake because neither type
    /// sufficiently overlaps with the other. If this was intentional, convert the expression to
    /// 'unknown' first.
    pub fn error_type_assertion_no_overlap(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        let source_type = self.widen_function_like_assertion_source(source_type);
        let source_str = self.format_type_for_assignability_message(source_type);
        let target_str = self.format_type_for_assignability_message(target_type);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
            &[&source_str, &target_str],
        );
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
