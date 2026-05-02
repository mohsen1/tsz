//! Diagnostic filtering and rollback helpers for speculative call checking.

use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::checkers::call::stable_call_recovery_return_type;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

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

    pub(super) fn overload_candidate_has_callback_body_errors(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        let speculative = self.ctx.speculative_diagnostics_since(snap);
        if speculative.is_empty() {
            return false;
        }
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
            }
        }
        false
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
            !callback_spans
                .iter()
                .any(|(start, end)| diag.start >= *start && diag.start < *end)
        });
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
}
