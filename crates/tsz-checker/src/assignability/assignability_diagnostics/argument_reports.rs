use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_assignable_or_report_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        let outcome = self.assignability_reason_relation_outcome(source, target);
        if outcome.related {
            return true;
        }

        // Use the canonical assign relation outcome so the weak-union hint is collected alongside
        // the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    ///
    /// Uses the canonical `RelationRequest` path for combined assignability +
    /// weak-union detection.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(arg_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(arg_idx, arg_idx) {
            return true;
        }
        if target == TypeId::NEVER && self.generic_indexed_access_argument_surface(source) {
            return true;
        }
        let outcome = self.call_arg_relation_outcome(source, target);
        let mut checker_only_mismatch = None;
        if outcome.related {
            let mismatch = self
                .checker_only_assignability_failure_reason(source, target)
                .is_some();
            if !mismatch {
                return true;
            }
            checker_only_mismatch = Some(mismatch);
        }
        if self.should_suppress_partial_self_argument_mismatch(source, target) {
            return true;
        }
        if self.should_suppress_self_referential_generic_function_arg_mismatch(source, target) {
            return true;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            source, target, arg_idx,
        ) {
            return true;
        }

        if self.should_skip_weak_union_error_with_outcome(source, target, arg_idx, Some(&outcome)) {
            return true;
        }
        // Conditional/generic callback contexts can narrow argument callback parameter
        // types to intersections involving type parameters (e.g. `number & T`).
        // In these cases, strict contravariant checking reports TS2345 even when the
        // concrete expected callback type is assignable to the narrowed callback.
        // tsc defers this mismatch.
        //
        // Only suppress when the source's parameter types contain type parameters
        // in an intersection with concrete types (indicating narrowing), not when
        // the parameters are standalone type parameters from an enclosing scope.
        // Without this restriction, `(x: T) => void` would be incorrectly accepted
        // for `(x: unknown) => void` just because `T <: unknown` holds in reverse.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            && !crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
            && crate::query_boundaries::assignability::is_callable_type(self.ctx.types, source)
            && crate::query_boundaries::assignability::is_callable_type(self.ctx.types, target)
            && !self.callable_has_own_generic_signatures(source)
            && self.call_arg_relation_outcome(target, source).related
            && self.callable_params_contain_type_param_intersection(source)
        {
            return true;
        }
        // Suppress TS2345 for callbacks with unannotated parameters that rely on
        // contextual typing. When a callback has unannotated parameters, its type
        // depends on the contextual type from the call site. If the contextual
        // typing wasn't properly applied during type inference, the callback's
        // inferred type may not match the expected type, causing false TS2345.
        // This handles cases like JSDoc @enum types where the callback parameter
        // should be contextually typed but the assignability check happens before
        // contextual typing is fully resolved.
        //
        // Only suppress when the target callable can actually contextually type
        // every parameter of the source callback. If the target signature has
        // fewer fixed parameters than the source callback (and no rest
        // parameter), contextual typing cannot supply types for the extra
        // source parameters, and the parameter-count mismatch ("Target
        // signature provides too few arguments") must surface as TS2345.
        let checker_only_mismatch = checker_only_mismatch.unwrap_or_else(|| {
            self.checker_only_assignability_failure_reason(source, target)
                .is_some()
        });
        if !checker_only_mismatch
            && self.arg_is_callback_with_unannotated_params(arg_idx)
            && self.target_can_contextually_type_callback_params(arg_idx, target)
        {
            return true;
        }
        // Before emitting TS2345 on the whole argument, try to elaborate
        // the error down to specific properties (TS2322) for object/array
        // literal arguments. tsc reports TS2322 on specific mismatched
        // properties rather than TS2345 on the whole argument.
        if self.try_elaborate_assignment_source_error(arg_idx, target) {
            return false;
        }
        if self.try_elaborate_callback_body_diagnostics(arg_idx, target) {
            return false;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }
}
