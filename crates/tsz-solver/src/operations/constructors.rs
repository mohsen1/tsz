//! Constructor (new) expression resolution.
//!
//! This module handles `new` expressions, resolving construct signatures
//! for classes and interfaces. It mirrors the function call resolution
//! but with construct-specific semantics (e.g., union strictness, mixin pattern).

use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{CallableShape, FunctionShape, TypeData, TypeId, TypeListId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Resolve a `new` expression (constructor call).
    ///
    /// This mirrors `resolve_call` but handles construct signatures instead of call signatures.
    /// Key differences from function calls:
    /// - Uses `construct_signatures` instead of `call_signatures`
    /// - For unions: ALL members must be constructable (stricter than function calls)
    /// - For intersections: Returns intersection of instance types (Mixin pattern)
    pub fn resolve_new(&mut self, type_id: TypeId, arg_types: &[TypeId]) -> CallResult {
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return CallResult::NotCallable { type_id },
        };

        match key {
            TypeData::Function(f_id) => {
                let shape = self.interner.function_shape(f_id);
                if shape.is_constructor {
                    self.resolve_function_call(shape.as_ref(), arg_types)
                } else {
                    // In TypeScript, `new f()` on a regular function (not arrow)
                    // is allowed and returns `any`, BUT only if it returns void.
                    // This matches TS2350 semantics.
                    match self.resolve_function_call(shape.as_ref(), arg_types) {
                        CallResult::Success(ret_type) => {
                            let ret_type =
                                crate::evaluation::evaluate::evaluate_type(self.interner, ret_type);
                            if ret_type != TypeId::VOID {
                                CallResult::NonVoidFunctionCalledWithNew
                            } else {
                                CallResult::VoidFunctionCalledWithNew
                            }
                        }
                        err => err,
                    }
                }
            }
            TypeData::Callable(c_id) => {
                let shape = self.interner.callable_shape(c_id);
                self.resolve_callable_new(shape.as_ref(), arg_types)
            }
            TypeData::Union(list_id) => self.resolve_union_new(type_id, list_id, arg_types),
            TypeData::Intersection(list_id) => {
                self.resolve_intersection_new(type_id, list_id, arg_types)
            }
            TypeData::Application(app_id) => {
                let evaluated = self.checker.evaluate_type(type_id);
                if evaluated != type_id {
                    self.resolve_new(evaluated, arg_types)
                } else {
                    let app = self.interner.type_application(app_id);
                    self.resolve_new(app.base, arg_types)
                }
            }
            TypeData::TypeParameter(param_info) => {
                if let Some(constraint) = param_info.constraint {
                    self.resolve_new(constraint, arg_types)
                } else {
                    CallResult::NotCallable { type_id }
                }
            }
            TypeData::Lazy(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::TypeQuery(_) => {
                // Resolve meta-types to their actual types before checking constructability.
                // Use checker.evaluate_type() which has a full resolver context,
                // rather than the standalone evaluate_type() with NoopResolver.
                // This is critical for Lazy(DefId) types (interfaces, type aliases)
                // which require the checker's resolver to look up their definitions.
                let resolved = self.checker.evaluate_type(type_id);
                if resolved != type_id {
                    self.resolve_new(resolved, arg_types)
                } else {
                    CallResult::NotCallable { type_id }
                }
            }
            _ => CallResult::NotCallable { type_id },
        }
    }

    /// Resolve a `new` expression on a Callable type.
    ///
    /// This handles classes and interfaces with construct signatures.
    fn resolve_callable_new(&mut self, shape: &CallableShape, arg_types: &[TypeId]) -> CallResult {
        if shape.construct_signatures.is_empty() {
            // If there are call signatures but no construct signatures (e.g. a method
            // accessed as a property), TypeScript allows `new` and returns `any`
            // matching JS semantics where any function can be used as a constructor,
            // BUT ONLY if it resolves to a signature that returns `void`. (TS2350)
            if !shape.call_signatures.is_empty() {
                match self.resolve_callable_call(shape, arg_types) {
                    CallResult::Success(ret_type) => {
                        let ret_type =
                            crate::evaluation::evaluate::evaluate_type(self.interner, ret_type);
                        if ret_type != TypeId::VOID {
                            return CallResult::NonVoidFunctionCalledWithNew;
                        }
                        return CallResult::VoidFunctionCalledWithNew;
                    }
                    err => return err,
                }
            }
            return CallResult::NotCallable {
                type_id: self.interner.callable(shape.clone()),
            };
        }

        if shape.construct_signatures.len() == 1 {
            let sig = &shape.construct_signatures[0];
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: true,
                is_method: false,
            };
            return self.resolve_function_call(&func, arg_types);
        }

        // Handle overloads (similar to resolve_callable_call)
        let mut failures = Vec::new();
        let mut all_arg_count_mismatches = true;
        let mut min_expected = usize::MAX;
        let mut max_expected = 0;
        let mut any_has_rest = false;
        let actual_count = arg_types.len();
        // Track single count-compatible overload that fails on types (see resolve_callable_call).
        let mut type_mismatch_count: usize = 0;
        let mut first_type_mismatch: Option<(usize, TypeId, TypeId, TypeId)> = None;
        let mut all_mismatches_identical = true;
        let mut all_mismatch_fallbacks_identical = true;
        let mut has_non_count_non_type_failure = false;
        // Also track this-type mismatches for TS2345 optimization (tsc reports TS2345 not TS2769
        // when all failures are identical this-type mismatches)
        let mut this_mismatch_count: usize = 0;
        let mut first_this_mismatch: Option<(TypeId, TypeId)> = None; // (expected, actual)
        let mut all_this_mismatches_identical = true;

        for sig in &shape.construct_signatures {
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: true,
                is_method: false,
            };

            match self.resolve_function_call(&func, arg_types) {
                CallResult::Success(ret) => return CallResult::Success(ret),
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    return CallResult::Success(return_type);
                }
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return,
                } => {
                    all_arg_count_mismatches = false;
                    type_mismatch_count += 1;
                    if type_mismatch_count == 1 {
                        first_type_mismatch = Some((index, expected, actual, fallback_return));
                    } else if let Some((
                        first_index,
                        first_expected,
                        first_actual,
                        first_fallback,
                    )) = first_type_mismatch
                    {
                        if (first_index, first_expected, first_actual) != (index, expected, actual)
                        {
                            all_mismatches_identical = false;
                        }
                        if first_fallback != fallback_return {
                            all_mismatch_fallbacks_identical = false;
                        }
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

        if all_arg_count_mismatches && !failures.is_empty() {
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

        // Same "best candidate" heuristic as resolve_callable_call.
        if !has_non_count_non_type_failure
            && type_mismatch_count > 0
            && all_mismatches_identical
            && let Some((index, expected, actual, fallback_return)) = first_type_mismatch
        {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return: if all_mismatch_fallbacks_identical {
                    fallback_return
                } else {
                    TypeId::ERROR
                },
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

        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(shape.clone()),
            arg_types: arg_types.to_vec(),
            failures,
            fallback_return: shape
                .construct_signatures
                .first()
                .map(|s| s.return_type)
                .unwrap_or(TypeId::ANY),
        }
    }

    /// Resolve a `new` expression on a union type.
    ///
    /// Uses the same three-phase approach as `resolve_union_call`:
    ///
    /// Phase 1: Arity check against the combined signature (max of all members'
    ///          required counts, intersection of param types, union of return types).
    /// Phase 2: Per-member resolution to collect actual return types.
    /// Phase 3: Validate arg types against the combined (intersected) param types.
    ///
    /// When no combined signature exists (any member has multiple/generic construct
    /// signatures), falls back to strict per-member semantics: ALL members must
    /// succeed for the union to succeed.
    fn resolve_union_new(
        &mut self,
        union_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // Compute a combined construct signature when all members have exactly one
        // non-generic construct signature. Intersects param types (contravariant)
        // and unions return types.
        let combined = self.try_compute_combined_union_construct_signature(&members);

        // Phase 1: Argument count validation using combined signature.
        if let Some(ref combined) = combined {
            if arg_types.len() < combined.min_required {
                return CallResult::ArgumentCountMismatch {
                    expected_min: combined.min_required,
                    expected_max: combined.max_allowed,
                    actual: arg_types.len(),
                };
            }
            if let Some(max) = combined.max_allowed
                && arg_types.len() > max
            {
                return CallResult::ArgumentCountMismatch {
                    expected_min: combined.min_required,
                    expected_max: combined.max_allowed,
                    actual: arg_types.len(),
                };
            }
        }

        // Phase 2: Per-member resolution to collect return types and failures.
        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            match self.resolve_new(member, arg_types) {
                CallResult::Success(ret) => {
                    return_types.push(ret);
                }
                CallResult::NotCallable { .. } => {
                    if combined.is_some() {
                        // Combined signature guarantees each member has a construct
                        // signature; NotCallable is unexpected — treat as full failure.
                        return CallResult::NotCallable {
                            type_id: union_type,
                        };
                    }
                    // When no combined signature, skip non-constructable members.
                }
                err => {
                    failures.push(err);
                }
            }
        }

        // Phase 3 (combined path): validate arg types against intersected param types.
        if let Some(ref combined) = combined {
            // When all members succeeded, return the union of their return types.
            if failures.is_empty() {
                let return_type = if return_types.len() == 1 {
                    return_types[0]
                } else {
                    self.interner.union(return_types)
                };
                return CallResult::Success(return_type);
            }

            // Validate each arg against the combined (intersected) parameter type.
            for (i, &arg_type) in arg_types.iter().enumerate() {
                if i < combined.param_types.len() {
                    let param_type = combined.param_types[i];
                    if !self.checker.is_assignable_to(arg_type, param_type) {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: param_type,
                            actual: arg_type,
                            fallback_return: combined.return_type,
                        };
                    }
                }
            }

            // All arg types passed; handle arity-only failures from per-member resolution.
            // (Can happen when a member has fewer params than the combined max allows.)
            let all_failures_are_arity = !failures.is_empty()
                && failures
                    .iter()
                    .all(|f| matches!(f, CallResult::ArgumentCountMismatch { .. }));

            if all_failures_are_arity && !return_types.is_empty() {
                // Some members succeeded, some failed on arity alone — combined
                // arity check passed, so the call is valid.
                return CallResult::Success(combined.return_type);
            }

            if all_failures_are_arity || failures.is_empty() {
                return CallResult::Success(combined.return_type);
            }

            // Mixed failures — propagate first failure.
            return failures
                .into_iter()
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                });
        }

        // No combined signature — strict per-member semantics: ALL members must succeed.
        if !return_types.is_empty() {
            if failures.is_empty() {
                let return_type = if return_types.len() == 1 {
                    return_types[0]
                } else {
                    self.interner.union(return_types)
                };
                return CallResult::Success(return_type);
            }
            // Some members failed — propagate first failure.
            return failures
                .into_iter()
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                });
        }

        if !failures.is_empty() {
            return failures
                .into_iter()
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                });
        }

        CallResult::NotCallable {
            type_id: union_type,
        }
    }

    /// Resolve a `new` expression on an intersection type.
    ///
    /// This handles the Mixin pattern: an intersection of constructors results in
    /// a constructor that returns the intersection of their instance types.
    ///
    /// e.g. `type Mixin = (new () => A) & (new () => B);`
    ///      `new Mixin()` -> `A & B`
    fn resolve_intersection_new(
        &mut self,
        intersection_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);
        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            // Try to resolve new on each member
            match self.resolve_new(member, arg_types) {
                CallResult::Success(ret) => {
                    return_types.push(ret);
                }
                CallResult::NotCallable { .. } => {
                    // Ignore non-constructable members in an intersection
                    // (e.g. Constructor & { staticProp: number })
                    continue;
                }
                err => {
                    // If it IS constructable but failed (e.g. arg mismatch), record it
                    failures.push(err);
                }
            }
        }

        if !return_types.is_empty() {
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            // Return intersection of all instance types (Mixin pattern)
            let intersection_result = self.interner.intersection(return_types);
            CallResult::Success(intersection_result)
        } else if !failures.is_empty() {
            // If we found constructors but they failed matching args, return the failure
            failures
                .into_iter()
                .next()
                .expect("failures is non-empty when no constituent is callable")
        } else {
            // No constructable members found
            CallResult::NotCallable {
                type_id: intersection_type,
            }
        }
    }
}
