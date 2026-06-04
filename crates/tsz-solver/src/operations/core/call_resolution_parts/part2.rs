impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Resolve a call to a simple function type.
    pub(crate) fn resolve_function_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // Handle generic functions FIRST so uninstantiated this_types don't fail assignability
        if !func.type_params.is_empty() {
            if let Some(result) = self.resolve_trivial_single_type_param_call(func, arg_types) {
                return result;
            }
            if let Some(func) = self.generic_function_shape_for_inference(func, arg_types) {
                let r = self.resolve_generic_call(&func, arg_types);
                return self.cache_generic_result(r);
            }
            let r = self.resolve_generic_call(func, arg_types);
            return self.cache_generic_result(r);
        }

        // Check `this` context if specified by the function shape.
        // IMPORTANT: Defer `this` errors to after argument checking — TSC reports
        // argument errors (TS2345) before `this` context errors (TS2684). A
        // declared `this: void` opts out (see `receiver_constraining_this_type`).
        let deferred_this_error =
            if let Some(expected_this) = receiver_constraining_this_type(func.this_type) {
                if let Some(actual_this) = self.actual_this_type {
                    if !self.checker.is_assignable_to(actual_this, expected_this) {
                        Some(CallResult::ThisTypeMismatch {
                            expected_this,
                            actual_this,
                            emit_not_callable: false,
                        })
                    } else {
                        None
                    }
                } else if !self.checker.is_assignable_to(TypeId::VOID, expected_this) {
                    Some(CallResult::ThisTypeMismatch {
                        expected_this,
                        actual_this: TypeId::VOID,
                        emit_not_callable: false,
                    })
                } else {
                    None
                }
            } else {
                None
            };

        // Check argument count
        let (min_args, max_args) = self.arg_count_bounds(&func.params);

        if arg_types.len() < min_args {
            // For variadic tuple rest params (e.g. `...args: [...T[], Required]`),
            // TSC checks assignability of the args-as-tuple against the rest param
            // type, producing TS2345 instead of TS2555. Detect this case and return
            // ArgumentTypeMismatch so the checker emits TS2345.
            if let Some(rest_param) = func.params.last().filter(|p| p.rest) {
                let rest_type = self.unwrap_readonly(rest_param.type_id);
                // `...args: never` means any call is invalid — TSC builds an empty
                // tuple and checks it against `never`, producing TS2345.
                let should_type_check = if rest_type == TypeId::NEVER {
                    true
                } else if let Some(TypeData::Tuple(elements)) = self.interner.lookup(rest_type) {
                    let elems = self.interner.tuple_list(elements);
                    elems.iter().any(|e| e.rest)
                } else {
                    false
                };
                if should_type_check {
                    // Build tuple type from actual args
                    let args_tuple_elems: Vec<TupleElement> = arg_types
                        .iter()
                        .map(|&t| TupleElement {
                            type_id: t,
                            name: None,
                            optional: false,
                            rest: false,
                        })
                        .collect();
                    let args_tuple = self.interner.tuple(args_tuple_elems);
                    return CallResult::ArgumentTypeMismatch {
                        index: 0,
                        expected: rest_type,
                        actual: args_tuple,
                        fallback_return: func.return_type,
                    };
                }
            }
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: max_args,
                actual: arg_types.len(),
            };
        }

        if let Some(max) = max_args
            && arg_types.len() > max
        {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: Some(max),
                actual: arg_types.len(),
            };
        }

        // Generic functions handled above

        if let Some(result) = self.check_argument_types(&func.params, arg_types, func.is_method) {
            return result;
        }

        // Even if arg count and individual arg types pass, a `...args: never` rest param
        // means no call is valid. TSC checks the args-as-tuple against `never`.
        if let Some(rest_param) = func.params.last().filter(|p| p.rest) {
            let rest_type = self.unwrap_readonly(rest_param.type_id);
            if rest_type == TypeId::NEVER {
                let rest_start = func.params.len().saturating_sub(1);
                let rest_args = &arg_types[rest_start.min(arg_types.len())..];
                let args_tuple_elems: Vec<TupleElement> = rest_args
                    .iter()
                    .map(|&t| TupleElement {
                        type_id: t,
                        name: None,
                        optional: false,
                        rest: false,
                    })
                    .collect();
                let args_tuple = self.interner.tuple(args_tuple_elems);
                return CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: rest_type,
                    actual: args_tuple,
                    fallback_return: func.return_type,
                };
            }
        }

        // Arguments validated successfully — now check deferred `this` error.
        if let Some(this_error) = deferred_this_error {
            return this_error;
        }

        CallResult::Success(func.return_type)
    }

    /// Resolve a call to a callable type (with overloads).
    pub(crate) fn resolve_callable_call(
        &mut self,
        callable: &CallableShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // If there are no call signatures at all, this type is not callable
        // (e.g., a class constructor without call signatures)
        if callable.call_signatures.is_empty() {
            return CallResult::NotCallable {
                type_id: self.interner.callable(callable.clone()),
            };
        }

        if callable.call_signatures.len() == 1 {
            let sig = &callable.call_signatures[0];
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            };
            return self.resolve_function_call(&func, arg_types);
        }

        // Try each call signature
        let mut failures = Vec::with_capacity(callable.call_signatures.len());
        let mut all_arg_count_mismatches = true;
        let mut min_expected = usize::MAX;
        let mut max_expected = 0;
        let mut any_has_rest = false;
        let actual_count = arg_types.len();
        let mut exact_expected_counts = FxHashSet::default();
        // Track if exactly one overload matched argument count but had a type mismatch.
        // When there is a single "count-compatible" overload that fails only on types,
        // tsc reports TS2345 (the inner type error) rather than TS2769 (no overload matched).
        let mut type_mismatch_count: usize = 0;
        let mut first_type_mismatch: Option<(usize, TypeId, TypeId)> = None; // (index, expected, actual)
        let mut all_mismatches_identical = true;
        let mut has_non_count_non_type_failure = false;
        // Also track this-type mismatches for TS2345 optimization (tsc reports TS2345 not TS2769
        // when all failures are identical this-type mismatches)
        let mut this_mismatch_count: usize = 0;
        let mut first_this_mismatch: Option<(TypeId, TypeId)> = None; // (expected, actual)
        let mut all_this_mismatches_identical = true;

        for sig in &callable.call_signatures {
            // Convert CallSignature to FunctionShape
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            };
            tracing::debug!("resolve_callable_call: signature = {sig:?}");

            match self.resolve_function_call(&func, arg_types) {
                CallResult::Success(ret) => return CallResult::Success(ret),
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => {
                    all_arg_count_mismatches = false;
                    type_mismatch_count += 1;
                    if type_mismatch_count == 1 {
                        first_type_mismatch = Some((index, expected, actual));
                    } else if first_type_mismatch != Some((index, expected, actual)) {
                        all_mismatches_identical = false;
                    }
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ),
                    );
                }
                CallResult::ArgumentCountMismatch {
                    expected_min,
                    expected_max,
                    actual,
                } => {
                    if expected_max.is_none() {
                        any_has_rest = true;
                    } else if expected_min == expected_max.unwrap_or(expected_min) {
                        exact_expected_counts.insert(expected_min);
                    }
                    let max = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(max);
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::argument_count_mismatch(
                            expected_min,
                            max,
                            actual,
                        ),
                    );
                }
                // Track this-type mismatches for TS2345 optimization (tsc reports TS2345 not TS2769
                // when all count-compatible overloads fail with the same this-type mismatch)
                CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this,
                    ..
                } => {
                    all_arg_count_mismatches = false;
                    this_mismatch_count += 1;
                    if this_mismatch_count == 1 {
                        first_this_mismatch = Some((expected_this, actual_this));
                    } else if first_this_mismatch != Some((expected_this, actual_this)) {
                        all_this_mismatches_identical = false;
                    }
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::this_type_mismatch(
                            expected_this,
                            actual_this,
                        ),
                    );
                }
                _ => {
                    all_arg_count_mismatches = false;
                    has_non_count_non_type_failure = true;
                }
            }
        }

        // If all signatures failed due to argument count mismatch, report TS2554 instead of TS2769
        if all_arg_count_mismatches && !failures.is_empty() {
            if !any_has_rest
                && !exact_expected_counts.is_empty()
                && !exact_expected_counts.contains(&actual_count)
            {
                let mut lower = None;
                let mut upper = None;
                for &count in &exact_expected_counts {
                    if count < actual_count {
                        lower = Some(lower.map_or(count, |prev: usize| prev.max(count)));
                    } else if count > actual_count {
                        upper = Some(upper.map_or(count, |prev: usize| prev.min(count)));
                    }
                }
                if let (Some(expected_low), Some(expected_high)) = (lower, upper) {
                    return CallResult::OverloadArgumentCountMismatch {
                        actual: actual_count,
                        expected_low,
                        expected_high,
                    };
                }
            }
            return CallResult::ArgumentCountMismatch {
                expected_min: min_expected,
                expected_max: if any_has_rest {
                    None
                } else if max_expected > min_expected {
                    Some(max_expected)
                } else {
                    Some(min_expected)
                },
                actual: actual_count,
            };
        }

        // If all type mismatches are identical (or there's exactly one), and no other failures occurred,
        // report TS2345 (the inner type error) instead of TS2769. This handles duplicate signatures
        // or overloads where the failing parameter has the exact same type in all matching overloads.
        if !has_non_count_non_type_failure
            && type_mismatch_count > 0
            && all_mismatches_identical
            && let Some((index, expected, actual)) = first_type_mismatch
        {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return: TypeId::ERROR,
            };
        }

        // If all this-type mismatches are identical (or there's exactly one), and no other failures
        // occurred, report TS2345 instead of TS2769. Use index 0 for the this-type mismatch.
        if !has_non_count_non_type_failure
            && this_mismatch_count > 0
            && all_this_mismatches_identical
            && type_mismatch_count == 0
            && let Some((expected_this, actual_this)) = first_this_mismatch
        {
            return CallResult::ArgumentTypeMismatch {
                index: 0,
                expected: expected_this,
                actual: actual_this,
                fallback_return: TypeId::ERROR,
            };
        }

        // If we got here, no signature matched.
        let fallback_return =
            overload_failure_return_type(self.interner, &callable.call_signatures);
        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(callable.clone()),
            arg_types: arg_types.to_vec(),
            failures,
            fallback_return,
        }
    }
}
