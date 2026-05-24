//! Diagnostic filtering and rollback helpers for speculative call checking.

use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::checkers::call::{
    rest_array_element_type_for_type, stable_call_recovery_return_type, tuple_elements_for_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ParamInfo, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) const fn should_preserve_speculative_call_diagnostic(
        diag: &crate::diagnostics::Diagnostic,
    ) -> bool {
        diag.code == diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS
            || diag.code == diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE
            // TS2454 (variable used before being assigned) is a semantic fact
            // about the variable's definite-assignment status, not a speculative
            // inference result from the call. It must survive call-expression
            // diagnostic rollbacks (round 1 → round 2, return-context re-check).
            || diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
            // TS2304/TS2552/TS2662/TS2663 (cannot find name variants) are semantic
            // facts about name resolution, not speculative inference results.
            // If a name is undefined, it is undefined regardless of which
            // overload is tried.
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
            // TS2872/TS2873 (always truthy/falsy) are purely syntactic facts
            // about expression truthiness, not speculative inference results.
            // They must survive call-expression diagnostic rollbacks.
            || diag.code == diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY
            || diag.code == diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY
            // TS2352 from an explicit type assertion is not an overload-candidate
            // failure. If the assertion itself has no overlap, tsc reports it even
            // when the surrounding overloaded call resolves through a catch-all.
            || diag.code == diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV
            // TS2304/TS2552 (Cannot find name / did you mean?) are name-resolution
            // facts that do not depend on the overload candidate being tried.
            // They must survive speculative rollbacks so undeclared identifiers
            // in argument expressions always produce an error, matching tsc.
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
    }

    pub(super) fn overload_candidate_has_hard_non_callback_arg_errors(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        self.ctx
            .speculative_diagnostics_since(snap)
            .iter()
            .any(|diag| {
                args.iter().any(|&arg_idx| {
                    let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                        return false;
                    };
                    let is_callback_arg = self.is_callback_like_argument(arg_idx);
                    !is_callback_arg && diag.start >= arg_node.pos && diag.start < arg_node.end
                })
            })
    }

    /// Check if any diagnostics were produced inside callback argument bodies
    /// since the snapshot.
    ///
    /// TypeScript rejects overload candidates when the callback body produces
    /// errors (e.g., a failing inner call like `.concat()`) even when the
    /// callback's return type structurally matches the expected type. Without
    /// this check, overloads that infer degenerate types (like `never[]`) can
    /// appear to succeed while hiding real type errors.
    /// Codes that indicate an inner call or type relation failed due to wrong
    /// type inference — these should cause overload candidate rejection.
    const CALLBACK_BODY_REJECTION_CODES: &'static [u32] = &[
        2322, // Type 'X' is not assignable to type 'Y'
        2345, // Argument of type 'X' is not assignable to parameter of type 'Y'
        2347, // Untyped function calls may not accept type arguments
        2339, // Property 'X' does not exist on type 'Y'
        2769, // No overload matches this call
    ];

    pub(crate) fn overload_candidate_has_callback_body_errors(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        let speculative = self.ctx.speculative_diagnostics_since(snap);
        for &arg_idx in args {
            let Some(_) = self.ctx.arena.get(arg_idx) else {
                continue;
            };
            let is_callback_arg = self.is_callback_like_argument(arg_idx);
            if !is_callback_arg {
                continue;
            }
            for (body_start, body_end) in self.callback_body_spans(arg_idx) {
                if speculative.iter().any(|diag| {
                    diag.start >= body_start
                        && diag.start < body_end
                        && Self::CALLBACK_BODY_REJECTION_CODES.contains(&diag.code)
                }) {
                    return true;
                }
                if self.ctx.no_overload_call_nodes.iter().any(|node_id| {
                    let idx = NodeIndex(*node_id);
                    self.ctx.arena.get(idx).is_some_and(|node| {
                        node.pos >= body_start
                            && node.pos < body_end
                            && !snap.no_overload_call_nodes.contains(node_id)
                    })
                }) {
                    return true;
                }
            }
        }
        false
    }

    /// Speculatively re-type contextually-sensitive callback arguments against
    /// the selected overload signature's parameter types and report whether that
    /// produces callback body errors.
    ///
    /// Overload selection types callbacks under a union of all candidate
    /// signatures. A union of function-typed parameters collapses the callback's
    /// own parameter to `any`, so body errors that tsc reports against the
    /// resolved signature (`o?.a.b` continuation typing, accessing a property that
    /// only exists under a different overload, an assignment whose right side has
    /// the wrong type) never surface during selection. Detecting them here lets
    /// the caller defer the candidate to the signature-specific pass, which
    /// re-checks and reports them. All speculative state is rolled back.
    pub(crate) fn selected_overload_callback_body_has_errors(
        &mut self,
        args: &[NodeIndex],
        func_type: TypeId,
    ) -> bool {
        let helper =
            crate::query_boundaries::common::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                func_type,
                self.ctx.compiler_options.no_implicit_any,
            );
        let param_types: Vec<Option<TypeId>> = (0..args.len())
            .map(|i| {
                self.contextual_parameter_type_for_call_with_env_from_expected(
                    func_type,
                    i,
                    args.len(),
                )
                .or_else(|| helper.get_parameter_type_for_call(i, args.len()))
                .map(|param_type| self.normalize_contextual_call_param_type(param_type))
            })
            .collect();

        let candidates: Vec<(NodeIndex, TypeId)> = args
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(i, arg_idx)| {
                if !self.is_callback_like_argument(arg_idx) {
                    return None;
                }
                let param_type = param_types.get(i).copied().flatten()?;
                if param_type == TypeId::ANY
                    || param_type == TypeId::UNKNOWN
                    || param_type == TypeId::ERROR
                {
                    return None;
                }
                // Only concrete parameter types are meaningful here. A generic
                // overload's parameter still mentions its type parameters (or
                // `infer` placeholders) at this point; the two-pass instantiation
                // machinery owns those callbacks, and re-typing against an
                // uninstantiated parameter produces spurious body errors.
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    param_type,
                ) || crate::query_boundaries::common::contains_infer_types(
                    self.ctx.types,
                    param_type,
                ) {
                    return None;
                }
                Some((arg_idx, param_type))
            })
            .collect();
        if candidates.is_empty() {
            return false;
        }

        // `rollback_full` restores diagnostics and caches but not `node_types`,
        // so save the callback args' cached entries and restore them afterwards
        // (mirroring `raw_block_body_callback_mismatch`).
        let snap = self.ctx.snapshot_full();
        let saved: Vec<(u32, Option<TypeId>)> = candidates
            .iter()
            .map(|&(arg_idx, _)| (arg_idx.0, self.ctx.node_types.get(&arg_idx.0).copied()))
            .collect();
        let diag_snap = self.ctx.snapshot_diagnostics();
        for &(arg_idx, param_type) in &candidates {
            self.invalidate_expression_for_contextual_retry(arg_idx);
            let request = TypingRequest::with_contextual_type(param_type);
            let _ = self.get_type_of_node_with_request(arg_idx, &request);
        }
        let has_errors = self.overload_candidate_has_callback_body_errors(args, &diag_snap);
        self.ctx.rollback_full(&snap);
        for (arg_idx, _) in &candidates {
            self.invalidate_expression_for_contextual_retry(*arg_idx);
        }
        for (node_id, saved_ty) in saved {
            match saved_ty {
                Some(ty) => {
                    self.ctx.node_types.insert(node_id, ty);
                }
                None => {
                    self.ctx.node_types.remove(&node_id);
                }
            }
        }
        has_errors
    }

    pub(super) fn type_is_or_constrained_to_top_rest_any_callable(&self, type_id: TypeId) -> bool {
        if let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, type_id)
        {
            return self.type_is_or_constrained_to_top_rest_any_callable(constraint);
        }
        let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            type_id,
        ) else {
            return false;
        };
        if shape.is_constructor || shape.params.len() != 1 || !shape.params[0].rest {
            return false;
        }
        let rest_type = shape.params[0].type_id;
        let rest_elem =
            rest_array_element_type_for_type(self.ctx.types, &self.ctx.definition_store, rest_type)
                .or_else(|| {
                    tuple_elements_for_type(self.ctx.types, rest_type).and_then(|elems| {
                        elems
                            .into_iter()
                            .find(|elem| elem.rest)
                            .map(|elem| elem.type_id)
                    })
                });
        rest_elem.is_some_and(|elem| elem == TypeId::ANY || elem == TypeId::UNKNOWN)
            && (shape.return_type == TypeId::ANY || shape.return_type == TypeId::UNKNOWN)
    }

    pub(super) fn overload_candidate_has_only_retained_generic_rest_any_callback_body_errors(
        &self,
        args: &[NodeIndex],
        params: &[ParamInfo],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        let speculative = self.ctx.speculative_diagnostics_since(snap);
        let param_for_arg = |index: usize| {
            params
                .get(index)
                .or_else(|| params.last().filter(|param| param.rest))
                .map(|param| param.type_id)
        };
        let arg_accepts_provisional_body_errors = |this: &Self, arg_index: usize, arg_idx| {
            this.explicit_generic_function_has_fully_annotated_signature(arg_idx)
                && param_for_arg(arg_index).is_some_and(|param_type| {
                    this.type_is_or_constrained_to_top_rest_any_callable(param_type)
                })
        };

        let mut found = false;
        for diag in speculative
            .iter()
            .filter(|diag| Self::CALLBACK_BODY_REJECTION_CODES.contains(&diag.code))
        {
            let Some((arg_index, &arg_idx)) = args.iter().enumerate().find(|&(_, &arg_idx)| {
                self.is_callback_like_argument(arg_idx)
                    && self
                        .callback_body_spans(arg_idx)
                        .into_iter()
                        .any(|(start, end)| diag.start >= start && diag.start < end)
            }) else {
                return false;
            };
            if !arg_accepts_provisional_body_errors(self, arg_index, arg_idx) {
                return false;
            }
            found = true;
        }

        for node_id in self
            .ctx
            .no_overload_call_nodes
            .iter()
            .filter(|node_id| !snap.no_overload_call_nodes.contains(node_id))
        {
            let idx = NodeIndex(*node_id);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            let Some((arg_index, &arg_idx)) = args.iter().enumerate().find(|&(_, &arg_idx)| {
                self.is_callback_like_argument(arg_idx)
                    && self
                        .callback_body_spans(arg_idx)
                        .into_iter()
                        .any(|(start, end)| node.pos >= start && node.pos < end)
            }) else {
                return false;
            };
            if !arg_accepts_provisional_body_errors(self, arg_index, arg_idx) {
                return false;
            }
            found = true;
        }

        found
    }

    pub(super) fn prune_callback_body_diagnostics(
        &mut self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) {
        // Pre-compute callback body spans before entering the mutable borrow
        // for rollback_diagnostics_filtered.
        let callback_spans: Vec<(u32, u32)> = args
            .iter()
            .flat_map(|&arg_idx| {
                let Some(_) = self.ctx.arena.get(arg_idx) else {
                    return Vec::new();
                };
                let is_callback_arg = self.is_callback_like_argument(arg_idx);
                if !is_callback_arg {
                    return Vec::new();
                }
                self.callback_body_spans(arg_idx)
            })
            .collect();
        self.ctx.rollback_diagnostics_filtered(snap, |diag| {
            if Self::should_preserve_speculative_call_diagnostic(diag) {
                return true;
            }
            !callback_spans
                .iter()
                .any(|(start, end)| diag.start >= *start && diag.start < *end)
        });
    }

    pub(super) fn prune_speculative_callback_body_diagnostics_for_accepted_overload(
        &mut self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) {
        let callback_spans: Vec<(u32, u32)> = args
            .iter()
            .flat_map(|&arg_idx| {
                let Some(_) = self.ctx.arena.get(arg_idx) else {
                    return Vec::new();
                };
                if !self.is_callback_like_argument(arg_idx) {
                    return Vec::new();
                }
                self.callback_body_spans(arg_idx)
            })
            .collect();
        self.ctx.rollback_diagnostics_filtered(snap, |diag| {
            if Self::should_preserve_speculative_call_diagnostic(diag) {
                return true;
            }
            let in_callback_body = callback_spans
                .iter()
                .any(|(start, end)| diag.start >= *start && diag.start < *end);
            if !in_callback_body {
                return true;
            }
            matches!(
                diag.code,
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                    | diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            )
        });
    }

    pub(super) fn callback_body_failure_span(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Option<(usize, tsz_solver::SourceSpan)> {
        args.iter().enumerate().find_map(|(index, &arg_idx)| {
            if !self.is_callback_like_argument(arg_idx) {
                return None;
            }
            let callback_idx = self.callback_function_index(arg_idx)?;
            let func = self
                .ctx
                .arena
                .get(callback_idx)
                .and_then(|node| self.ctx.arena.get_function(node))?;
            let body = self.ctx.arena.get(func.body)?;
            let body_start = body.pos;
            let body_end = body.end;
            let has_failed_diagnostic = self
                .ctx
                .speculative_diagnostics_since(snap)
                .iter()
                .any(|diag| diag.start >= body_start && diag.start < body_end);
            let has_failed_no_overload_marker =
                self.ctx.no_overload_call_nodes.iter().any(|node_id| {
                    let idx = NodeIndex(*node_id);
                    self.ctx.arena.get(idx).is_some_and(|node| {
                        node.pos >= body_start
                            && node.pos < body_end
                            && !snap.no_overload_call_nodes.contains(node_id)
                    })
                });
            if !has_failed_diagnostic && !has_failed_no_overload_marker {
                return None;
            }
            Some((
                index,
                tsz_solver::SourceSpan::new(
                    self.ctx.file_name.clone(),
                    body.pos,
                    body.end.saturating_sub(body.pos),
                ),
            ))
        })
    }

    pub(super) fn callback_body_no_overload_diagnostics_since(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        let callback_spans: Vec<(u32, u32)> = args
            .iter()
            .copied()
            .filter(|&arg_idx| self.is_callback_like_argument(arg_idx))
            .flat_map(|arg_idx| self.callback_body_spans(arg_idx))
            .collect();
        if callback_spans.is_empty() {
            return Vec::new();
        }

        self.ctx
            .speculative_diagnostics_since(snap)
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                    && callback_spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
            })
            .cloned()
            .collect()
    }

    fn raw_block_body_callback_mismatch(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        args.iter().enumerate().find_map(|(index, &arg_idx)| {
            let _arg_node = self.ctx.arena.get(arg_idx)?;
            if !self.is_callback_like_argument(arg_idx) {
                return None;
            }
            let callback_idx = self.callback_function_index(arg_idx)?;
            let callback_relies_on_contextual_param_types = self
                .ctx
                .implicit_any_contextual_closures
                .contains(&callback_idx);
            let func = self
                .ctx
                .arena
                .get(callback_idx)
                .and_then(|node| self.ctx.arena.get_function(node))?;
            let callback_span = self
                .ctx
                .arena
                .get(callback_idx)
                .map(|node| (node.pos, node.end))?;
            let body = self.ctx.arena.get(func.body)?;
            if body.kind != syntax_kind_ext::BLOCK {
                return None;
            }
            let expected = expected_for_index(self, index)?;
            if expected == TypeId::ERROR || expected == TypeId::UNKNOWN || expected == TypeId::ANY {
                return None;
            }
            let expected_is_concrete = expected != TypeId::ANY
                && expected != TypeId::UNKNOWN
                && expected != TypeId::ERROR
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    expected,
                )
                && !crate::query_boundaries::common::contains_infer_types(self.ctx.types, expected);
            let snap = self.ctx.snapshot_full();
            // Save the node_types entry — rollback_full does not restore
            // node_types, so speculative recomputation below would otherwise
            // overwrite a contextually-typed cached value with an
            // uncontextualized one.
            let saved_node_type = self.ctx.node_types.get(&arg_idx.0).copied();
            let original_actual = saved_node_type.unwrap_or_else(|| self.get_type_of_node(arg_idx));
            self.invalidate_expression_for_contextual_retry(arg_idx);
            self.ctx.daa_error_nodes.remove(&arg_idx.0);
            self.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
            let diag_snap = self.ctx.snapshot_diagnostics();
            let callback_declares_own_types = func.type_annotation.is_some()
                || func.parameters.nodes.iter().any(|&param_idx| {
                    self.ctx
                        .arena
                        .get(param_idx)
                        .and_then(|node| self.ctx.arena.get_parameter(node))
                        .is_some_and(|param| param.type_annotation.is_some())
                });
            let recheck_request = if callback_declares_own_types {
                TypingRequest::NONE
            } else {
                TypingRequest::with_contextual_type(expected)
            };
            let actual = self.get_type_of_node_with_request(arg_idx, &recheck_request);
            let refined_actual = if self
                .target_has_concrete_return_context_for_generic_refinement(expected)
            {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    actual, expected,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(actual, expected)
            };
            let speculative = self.ctx.speculative_diagnostics_since(&diag_snap);
            let has_callback_assignability_diag = |diag: &crate::diagnostics::Diagnostic| {
                self.callback_body_spans(arg_idx)
                    .iter()
                    .any(|(start, end)| diag.start >= *start && diag.start < *end)
                    || ((diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        || diag.code
                            == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE)
                        && diag.start >= callback_span.0
                        && diag.start < callback_span.1)
            };
            let has_callback_body_diagnostic = self.ctx.diagnostics.iter().any(
                has_callback_assignability_diag,
            ) || speculative.iter().any(has_callback_assignability_diag);
            self.ctx.rollback_full(&snap);
            // Restore the node_types entry so the contextually-typed result
            // is preserved for subsequent lookups.
            if let Some(saved) = saved_node_type {
                self.ctx.node_types.insert(arg_idx.0, saved);
            } else {
                self.ctx.node_types.remove(&arg_idx.0);
            }
            if callback_relies_on_contextual_param_types
                && self.callback_type_params_are_unresolved(actual)
                && self.callback_type_params_are_unresolved(original_actual)
            {
                return None;
            }
            let recovery_actual = if stable_call_recovery_return_type(self.ctx.types, refined_actual)
                .is_some()
            {
                refined_actual
            } else {
                original_actual
            };
            let is_generator_callback = func.asterisk_token;
            let (has_return_type_mismatch, has_generator_component_mismatch) =
                stable_call_recovery_return_type(self.ctx.types, recovery_actual)
                    .zip(stable_call_recovery_return_type(self.ctx.types, expected))
                    .map(|(actual_return, expected_return)| {
                        // For yield and return components, the actual type was computed
                        // WITHOUT contextual typing, so literal types get widened
                        // (e.g., `yield 10` produces `number` instead of `10`).
                        // When the expected type comes from const type parameter
                        // inference (e.g., `Generator<10>`), the narrow expected type
                        // won't match the widened actual. To avoid false positives,
                        // also check the reverse direction: if the expected (narrow)
                        // type is assignable to the actual (widened) type, the
                        // mismatch is just due to widening, not a real error.
                        let generator_component_mismatch = self
                            .get_generator_yield_type_argument(actual_return)
                            .zip(self.get_generator_yield_type_argument(expected_return))
                            .is_some_and(|(actual_yield, expected_yield)| {
                                !self.is_assignable_to(actual_yield, expected_yield)
                                    && !self.is_assignable_to(expected_yield, actual_yield)
                            })
                            || self
                                .get_generator_return_type_argument(actual_return)
                                .zip(self.get_generator_return_type_argument(expected_return))
                                .is_some_and(|(actual_gen_return, expected_gen_return)| {
                                    !self.is_assignable_to(actual_gen_return, expected_gen_return)
                                        && !self.is_assignable_to(
                                            expected_gen_return,
                                            actual_gen_return,
                                        )
                                })
                            || self
                                .get_generator_next_type_argument(actual_return)
                                .zip(self.get_generator_next_type_argument(expected_return))
                                .is_some_and(|(actual_next, expected_next)| {
                                    !self.is_assignable_to(expected_next, actual_next)
                                });

                        // When the expected return type is `void`, there is never
                        // a return type mismatch — void return means "ignore the
                        // return value", so any actual return type is acceptable.
                        // This is the function-level void-return-substitutability
                        // rule, which differs from type-level `is_assignable_to`.
                        //
                        // For generator callbacks: when the per-component checks
                        // (yield, return, next) all pass, trust them and skip the
                        // overall `is_assignable_to` on the full generator
                        // Application type.  The overall check can produce false
                        // positives because the callback is re-evaluated WITHOUT
                        // contextual type, so TNext defaults to `unknown`.  The
                        // solver then checks TNext covariantly (`unknown </: number`)
                        // instead of contravariantly, causing a spurious mismatch.
                        // The component check already handles TNext contravariantly
                        // (line: `is_assignable_to(expected_next, actual_next)`),
                        // so it is the more accurate signal for generators.
                        // When the expected return type contains unresolved
                        // type parameters (e.g., `U` from a generic overload
                        // `reduce<U>(fn: (a: U) => U, init: U): U`), skip the
                        // return type mismatch check.  The type parameter is
                        // resolved via inference during generic call resolution,
                        // not by direct assignability.  Checking `number[]` vs
                        // `U` would always fail, causing a false TS2769.
                        let expected_return_has_type_params =
                            crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                expected_return,
                            );
                        let return_type_mismatch =
                            if (is_generator_callback && !generator_component_mismatch)
                                || expected_return_has_type_params
                            {
                                false
                            } else {
                                generator_component_mismatch
                                    || (expected_return != TypeId::VOID
                                        && !self.is_assignable_to(actual_return, expected_return))
                            };
                        (return_type_mismatch, generator_component_mismatch)
                    })
                    .unwrap_or((false, false));
            let should_force_argument_mismatch = if is_generator_callback {
                (has_callback_body_diagnostic
                    || (expected_is_concrete && has_generator_component_mismatch))
                    && has_return_type_mismatch
            } else {
                has_return_type_mismatch
            };
            should_force_argument_mismatch.then_some((index, recovery_actual, expected))
        })
    }

    pub(crate) fn current_block_body_callback_return_mismatch_arg(
        &mut self,
        args: &[NodeIndex],
        expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        self.raw_block_body_callback_mismatch(args, expected_for_index)
    }

    pub(crate) fn current_binding_pattern_callback_unknown_context_arg(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        args.iter().enumerate().find_map(|(index, &arg_idx)| {
            if !self.is_callback_like_argument(arg_idx) {
                return None;
            }
            let func = self
                .callback_function_index(arg_idx)
                .and_then(|idx| self.ctx.arena.get(idx))
                .and_then(|node| self.ctx.arena.get_function(node))?;
            let expected = expected_for_index(self, index)?;
            let expected_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected,
            )?;

            let has_unknown_binding_pattern =
                func.parameters
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(param_index, &param_idx)| {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            return false;
                        };
                        if param.type_annotation.is_some() {
                            return false;
                        }
                        let has_binding_pattern =
                            self.ctx.arena.get(param.name).is_some_and(|name_node| {
                                name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            });
                        if !has_binding_pattern {
                            return false;
                        }
                        expected_shape
                            .params
                            .get(param_index)
                            .map(|param| param.type_id)
                            .or_else(|| {
                                let last = expected_shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                            .is_some_and(|param_type| {
                                let unconstrained_type_param =
                                    crate::query_boundaries::common::type_parameter_constraint(
                                        self.ctx.types,
                                        param_type,
                                    )
                                    .is_none_or(|constraint| {
                                        constraint == TypeId::UNKNOWN
                                    || constraint == TypeId::ERROR
                                    || crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        constraint,
                                    )
                                    || crate::query_boundaries::common::contains_infer_types(
                                        self.ctx.types,
                                        constraint,
                                    )
                                    });
                                param_type == TypeId::UNKNOWN
                                    || param_type == TypeId::ERROR
                                    || crate::query_boundaries::common::contains_infer_types(
                                        self.ctx.types,
                                        param_type,
                                    )
                                    || (crate::query_boundaries::common::is_type_parameter_like(
                                        self.ctx.types,
                                        param_type,
                                    ) && unconstrained_type_param)
                            })
                    });

            if !has_unknown_binding_pattern {
                return None;
            }

            let actual = self.get_type_of_node_with_request(arg_idx, &TypingRequest::NONE);
            Some((index, actual, expected))
        })
    }

    pub(crate) fn collect_non_callback_diagnostics_between(
        &self,
        args: &[NodeIndex],
        from_snap: &crate::context::speculation::DiagnosticSnapshot,
        to_snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        self.ctx
            .diagnostics_between(from_snap, to_snap)
            .iter()
            .filter(|diag| {
                if Self::should_preserve_speculative_call_diagnostic(diag) {
                    return true;
                }
                !args.iter().any(|&arg_idx| {
                    let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                        return false;
                    };
                    // Object literal arguments with methods have body diagnostics
                    // that depend on contextual typing (e.g., ThisType<T> markers).
                    // Filter them like callback body diagnostics so they don't
                    // persist when a different overload resolves successfully.
                    let is_context_sensitive_arg = self.is_callback_like_argument(arg_idx)
                        || arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || arg_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
                    if !is_context_sensitive_arg {
                        return false;
                    }
                    if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        // For object literals, filter diagnostics within the entire span
                        diag.start >= arg_node.pos && diag.start < arg_node.end
                    } else {
                        self.callback_body_spans(arg_idx)
                            .iter()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end)
                    }
                })
            })
            .cloned()
            .collect()
    }

    pub(crate) fn collect_non_callback_and_body_assignability_diagnostics_between(
        &self,
        args: &[NodeIndex],
        from_snap: &crate::context::speculation::DiagnosticSnapshot,
        to_snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        self.ctx
            .diagnostics_between(from_snap, to_snap)
            .iter()
            .filter(|diag| {
                !args.iter().any(|&arg_idx| {
                    let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                        return false;
                    };
                    let is_callback_arg = self.is_callback_like_argument(arg_idx);
                    if is_callback_arg
                        && self
                            .callback_body_spans(arg_idx)
                            .iter()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end)
                    {
                        return diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                            && diag.code
                                != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                    }
                    if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        return diag.start >= arg_node.pos && diag.start < arg_node.end;
                    }
                    false
                })
            })
            .cloned()
            .collect()
    }

    pub(crate) fn preserved_speculative_call_diagnostics(
        &self,
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        self.ctx
            .speculative_diagnostics_since(snap)
            .iter()
            .filter(|diag| Self::should_preserve_speculative_call_diagnostic(diag))
            .cloned()
            .collect()
    }

    pub(crate) fn extend_unique_diagnostics(
        &self,
        dest: &mut Vec<crate::diagnostics::Diagnostic>,
        source: impl IntoIterator<Item = crate::diagnostics::Diagnostic>,
    ) {
        let mut seen = rustc_hash::FxHashSet::default();
        for diag in dest.iter() {
            seen.insert(self.ctx.diagnostic_dedup_key(diag));
        }
        for diag in source {
            let key = self.ctx.diagnostic_dedup_key(&diag);
            if seen.insert(key) {
                dest.push(diag);
            }
        }
    }

    pub(super) fn diagnostics_for_overload_mismatch_argument_between(
        &self,
        args: &[NodeIndex],
        index: usize,
        from_snap: &crate::context::speculation::DiagnosticSnapshot,
        to_snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        let Some(&arg_idx) = args.get(index) else {
            return Vec::new();
        };
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return Vec::new();
        };

        self.ctx
            .diagnostics_between(from_snap, to_snap)
            .iter()
            .filter(|diag| diag.start >= arg_node.pos && diag.start < arg_node.end)
            .cloned()
            .collect()
    }

    /// Snapshot a non-generic overload that matched at the signature level but
    /// failed only inside a callback body, so it can be committed later if no
    /// other overload resolves cleanly. See `commit_callback_body_only_candidate`.
    pub(super) fn build_callback_body_only_candidate(
        &self,
        args: &[NodeIndex],
        overload_diag: &crate::context::speculation::DiagnosticSnapshot,
        candidate_snap: &crate::context::speculation::DiagnosticSnapshot,
        arg_types: Vec<TypeId>,
        return_type: TypeId,
        selected_type_predicate: super::SelectedTypePredicate,
    ) -> (
        super::OverloadResolution,
        crate::context::NodeTypeCache,
        Vec<crate::diagnostics::Diagnostic>,
    ) {
        let candidate_end = self.ctx.snapshot_diagnostics();
        let preserved_first_pass =
            self.collect_non_callback_diagnostics_between(args, overload_diag, candidate_snap);
        let candidate_diags: Vec<_> = self
            .ctx
            .diagnostics_between(candidate_snap, &candidate_end)
            .to_vec();
        let mut merged = self.preserved_speculative_call_diagnostics(overload_diag);
        self.extend_unique_diagnostics(&mut merged, preserved_first_pass);
        self.extend_unique_diagnostics(&mut merged, candidate_diags);
        let resolution = super::OverloadResolution {
            arg_types,
            result: crate::query_boundaries::common::CallResult::Success(return_type),
            selected_type_predicate,
        };
        (resolution, self.ctx.node_types.clone(), merged)
    }

    /// Commit the captured callback-body-only candidate: restore diagnostics to
    /// the candidate's merged set (which retains the callback body errors) and
    /// install its node types. tsc reports those body diagnostics against the
    /// selected overload rather than silently recovering.
    pub(super) fn commit_callback_body_only_candidate(
        &mut self,
        candidate: (
            super::OverloadResolution,
            crate::context::NodeTypeCache,
            Vec<crate::diagnostics::Diagnostic>,
        ),
        overload_snap: &crate::context::speculation::FullSnapshot,
        original_node_types: &mut crate::context::NodeTypeCache,
    ) -> super::OverloadResolution {
        let (resolution, sig_node_types, merged_diags) = candidate;
        self.ctx
            .rollback_and_replace_diagnostics(&overload_snap.diag, merged_diags);
        self.ctx
            .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
        self.ctx.node_types = std::mem::take(original_node_types);
        self.ctx.node_types.merge_owned(sig_node_types);
        resolution
    }
}
