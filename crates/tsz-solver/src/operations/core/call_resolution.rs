//! Call resolution logic for `CallEvaluator`.
//!
//! Contains `resolve_call`, `resolve_union_call`, `resolve_intersection_call`,
//! `resolve_function_call`, `resolve_callable_call`, and related helpers,
//! plus the free-function entry points that construct a `CallEvaluator` and
//! delegate.

use super::call_evaluator::{
    AssignabilityChecker, CallEvaluator, CallResult, CallWithCheckerResult, CombinedUnionSignature,
    UnionCallSignatureCompatibility,
};
use crate::instantiation::instantiate::TypeSubstitution;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, IntrinsicKind, ParamInfo, TupleElement, TypeData,
    TypeId, TypeListId,
};
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::FxHashSet;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Resolve a function call: func(args...) -> result
    ///
    /// This is pure type logic - no AST nodes, just types in and types out.
    pub fn resolve_call(&mut self, func_type: TypeId, arg_types: &[TypeId]) -> CallResult {
        self.last_instantiated_predicate = None;
        self.last_instantiated_params = None;
        // Look up the function shape
        let key = match self.interner.lookup(func_type) {
            Some(k) => k,
            None => return CallResult::NotCallable { type_id: func_type },
        };

        match key {
            TypeData::Function(f_id) => {
                let shape = self.interner.function_shape(f_id);
                self.resolve_function_call(shape.as_ref(), arg_types)
            }
            TypeData::Callable(c_id) => {
                let shape = self.interner.callable_shape(c_id);
                self.resolve_callable_call(shape.as_ref(), arg_types)
            }
            TypeData::Union(list_id) => {
                // Handle union types: if all members are callable with compatible signatures,
                // the union is callable
                self.resolve_union_call(func_type, list_id, arg_types)
            }
            TypeData::Intersection(list_id) => {
                // Handle intersection types: if any member is callable, use that
                // This handles cases like: Function & { prop: number }
                self.resolve_intersection_call(func_type, list_id, arg_types)
            }
            TypeData::Application(_app_id) => {
                // Handle Application types (e.g., GenericCallable<string>)
                // Evaluate the application type to properly instantiate its base type with arguments
                let evaluated = self.checker.evaluate_type(func_type);
                if evaluated != func_type {
                    self.resolve_call(evaluated, arg_types)
                } else {
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            TypeData::TypeParameter(param_info) => {
                // For type parameters with callable constraints (e.g., T extends { (): string }),
                // resolve the call using the constraint type
                if let Some(constraint) = param_info.constraint {
                    self.resolve_call(constraint, arg_types)
                } else {
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            TypeData::Conditional(cond_id) => {
                // First try to evaluate the conditional type to a concrete type.
                let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, func_type);
                if resolved != func_type {
                    return self.resolve_call(resolved, arg_types);
                }
                // For deferred conditional types (containing type parameters that
                // can't be resolved yet), check if both branches are callable.
                // tsc extracts call signatures from both branches of a deferred
                // conditional type. For example:
                //   type Q<T> = number extends T ? (n: number) => void : never;
                // When T is unknown, Q<T> is still callable because the true branch
                // is callable and the false branch is `never`.
                let cond = self.interner.conditional_type(cond_id);
                let true_type = cond.true_type;
                let false_type = cond.false_type;
                let true_is_never = true_type == TypeId::NEVER;
                let false_is_never = false_type == TypeId::NEVER;
                if true_is_never && false_is_never {
                    CallResult::NotCallable { type_id: func_type }
                } else if false_is_never {
                    self.resolve_call(true_type, arg_types)
                } else if true_is_never {
                    self.resolve_call(false_type, arg_types)
                } else {
                    // Both branches are non-never — try calling the true branch.
                    // If that succeeds, also try the false branch and union their
                    // return types, matching tsc behavior.
                    let true_result = self.resolve_call(true_type, arg_types);
                    let false_result = self.resolve_call(false_type, arg_types);
                    match (&true_result, &false_result) {
                        (CallResult::Success(true_ret), CallResult::Success(false_ret)) => {
                            CallResult::Success(self.interner.union2(*true_ret, *false_ret))
                        }
                        (CallResult::Success(_), _) | (_, CallResult::Success(_)) => {
                            // One branch callable, other not — still callable
                            // (the non-callable branch may be unreachable)
                            match true_result {
                                CallResult::Success(_) => true_result,
                                _ => false_result,
                            }
                        }
                        _ => CallResult::NotCallable { type_id: func_type },
                    }
                }
            }
            TypeData::Lazy(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::TypeQuery(_) => {
                // Resolve meta-types to their actual types before checking callability.
                // This handles cases like index access types like T["method"],
                // and mapped types.
                //
                // Use checker.evaluate_type() which has a full resolver context,
                // rather than the standalone evaluate_type() with NoopResolver.
                // This is needed because IndexAccess types like T[K] where
                // T extends Record<K, F> require resolving Lazy(DefId) references
                // (e.g., Record's DefId) to expand Application types into their
                // structural form (mapped types) before the index can be resolved.
                let resolved = self.checker.evaluate_type(func_type);
                if resolved != func_type {
                    self.resolve_call(resolved, arg_types)
                } else {
                    // If evaluation couldn't resolve (e.g., IndexAccess with generic
                    // index), try using the base constraint. For `Obj[K]` where
                    // `K extends "a" | "b"`, substitute the constraint to get
                    // `Obj["a" | "b"]` which can be evaluated to a concrete callable type.
                    // This matches tsc's getBaseConstraintOfType for indexed access types.
                    if let Some(TypeData::IndexAccess(obj, idx)) = self.interner.lookup(func_type) {
                        // Try to get the base constraint of the index type.
                        // For a TypeParameter K extends C, use C.
                        // For an Intersection containing a TypeParameter, use
                        // the TypeParameter's constraint (which is a superset).
                        let constraint = self.get_index_constraint(idx);
                        if let Some(constraint_type) = constraint {
                            let eval_constraint = self.checker.evaluate_type(constraint_type);
                            let constrained_access =
                                self.interner.index_access(obj, eval_constraint);
                            let constrained = self.checker.evaluate_type(constrained_access);
                            if constrained != func_type {
                                return self.resolve_call(constrained, arg_types);
                            }
                        }
                    }
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            // The `Function` intrinsic type is callable in TypeScript and returns `any`.
            // This matches tsc behavior: `declare const f: Function; f()` is valid.
            TypeData::Intrinsic(IntrinsicKind::Function | IntrinsicKind::Any) => {
                CallResult::Success(TypeId::ANY)
            }
            // `any` is callable and returns `any`
            // `error` propagates as error
            TypeData::Error => CallResult::Success(TypeId::ERROR),
            _ => CallResult::NotCallable { type_id: func_type },
        }
    }

    /// Resolve a call on a union type.
    ///
    /// This handles cases like:
    /// - `(() => void) | (() => string)` - all members callable
    /// - `string | (() => void)` - mixed callable/non-callable (returns `NotCallable`)
    ///
    /// When all union members are callable with compatible signatures, this returns
    /// a union of their return types.
    fn union_call_signature_bounds(
        &mut self,
        members: &[TypeId],
    ) -> UnionCallSignatureCompatibility {
        let mut has_rest = false;
        let mut has_non_rest = false;
        let mut min_required = 0usize;
        let mut max_allowed: Option<usize> = Some(0);
        let mut found_callable = false;
        let mut signatures: Vec<Vec<ParamInfo>> = Vec::new();

        for &member in members.iter() {
            let Some(signature) = self.extract_union_call_signature(member) else {
                return UnionCallSignatureCompatibility::Unknown;
            };
            found_callable = true;
            signatures.push(signature);
        }

        if !found_callable || signatures.is_empty() {
            return UnionCallSignatureCompatibility::Unknown;
        }

        let max_params = signatures.iter().map(Vec::len).max().unwrap_or_default();

        for index in 0..max_params {
            let mut saw_required = false;
            let mut saw_optional = false;
            let mut saw_rest = false;
            let mut saw_absent = false;
            let mut saw_non_rest = false;

            for signature in &signatures {
                if index >= signature.len() {
                    saw_absent = true;
                    continue;
                }

                let param = &signature[index];
                if param.rest {
                    saw_rest = true;
                    if index != signature.len() - 1 {
                        return UnionCallSignatureCompatibility::Unknown;
                    }
                    saw_non_rest = false;
                } else {
                    saw_non_rest = true;
                    if param.is_required() {
                        saw_required = true;
                    } else {
                        saw_optional = true;
                    }
                }
            }

            if saw_rest && saw_non_rest {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required && saw_absent {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required && saw_optional && index > 0 {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required {
                min_required += 1;
                max_allowed = max_allowed.map(|max| max + 1);
            } else if saw_optional || saw_rest || saw_absent {
                max_allowed = max_allowed.and_then(|max| max.checked_add(1));
            }

            if saw_rest {
                has_rest = true;
            }
            if saw_non_rest {
                has_non_rest = true;
            }
        }

        let max_allowed = if has_rest && has_non_rest {
            return UnionCallSignatureCompatibility::Incompatible;
        } else if has_rest {
            None
        } else {
            max_allowed
        };

        UnionCallSignatureCompatibility::Compatible {
            min_required,
            max_allowed,
        }
    }

    fn extract_union_call_signature(&mut self, member: TypeId) -> Option<Vec<ParamInfo>> {
        let member = self.normalize_union_member(member);
        match self.interner.lookup(member) {
            Some(TypeData::Function(func_id)) => {
                let function = self.interner.function_shape(func_id);
                if !function.type_params.is_empty() {
                    return None;
                }
                Some(self.normalize_union_signature_params(&function.params))
            }
            Some(TypeData::Callable(callable_id)) => {
                let callable = self.interner.callable_shape(callable_id);
                if callable.call_signatures.len() != 1 {
                    return None;
                }
                let signature = &callable.call_signatures[0];
                if !signature.type_params.is_empty() {
                    return None;
                }
                Some(self.normalize_union_signature_params(&signature.params))
            }
            _ => None,
        }
    }

    fn normalize_union_signature_params(&mut self, params: &[ParamInfo]) -> Vec<ParamInfo> {
        params
            .iter()
            .flat_map(|param| {
                let mut normalized = *param;
                if normalized.rest {
                    normalized.type_id = match self.interner.lookup(normalized.type_id) {
                        Some(
                            TypeData::Application(_)
                            | TypeData::Mapped(_)
                            | TypeData::Intersection(_)
                            | TypeData::Conditional(_)
                            | TypeData::Lazy(_),
                        ) => self.checker.evaluate_type(normalized.type_id),
                        _ => normalized.type_id,
                    };
                }
                crate::type_queries::unpack_tuple_rest_parameter(self.interner, &normalized)
            })
            .collect()
    }

    fn is_single_signature_callable_member(&self, member: TypeId) -> bool {
        let member = self.normalize_union_member(member);
        match self.interner.lookup(member) {
            Some(TypeData::Function(_)) => true,
            Some(TypeData::Callable(callable_id)) => {
                let callable = self.interner.callable_shape(callable_id);
                callable.call_signatures.len() == 1
            }
            _ => false,
        }
    }

    /// Try to compute a combined call signature for a union type.
    ///
    /// In TypeScript, when all members of a union have exactly one call signature
    /// (non-generic), the union is callable with a combined signature where:
    /// - Parameter types are intersected (contravariant position)
    /// - Return types are unioned
    /// - Required param count is the max across all members
    ///
    /// Returns `None` if any member is not callable or has multiple/generic signatures.
    fn try_compute_combined_union_signature(
        &mut self,
        members: &[TypeId],
    ) -> Option<CombinedUnionSignature> {
        if members.is_empty() {
            return None;
        }

        // Collect single signatures from each member: (params, return_type, has_rest)
        let mut all_signatures: Vec<(Vec<ParamInfo>, TypeId, bool)> = Vec::new();

        for &member in members {
            let member = self.normalize_union_member(member);
            match self.interner.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let function = self.interner.function_shape(func_id);
                    if !function.type_params.is_empty() {
                        return None; // generic functions need separate handling
                    }
                    let params = self.normalize_union_signature_params(&function.params);
                    let has_rest = params.iter().any(|p| p.rest);
                    all_signatures.push((params, function.return_type, has_rest));
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    if callable.call_signatures.len() != 1 {
                        return None; // multiple overloads need separate handling
                    }
                    let sig = &callable.call_signatures[0];
                    if !sig.type_params.is_empty() {
                        return None;
                    }
                    let params = self.normalize_union_signature_params(&sig.params);
                    let has_rest = params.iter().any(|p| p.rest);
                    all_signatures.push((params, sig.return_type, has_rest));
                }
                _ => return None, // not callable
            }
        }

        if all_signatures.is_empty() {
            return None;
        }

        // Determine max param count for iterating all positions
        let max_param_count = all_signatures
            .iter()
            .map(|(params, _, _)| params.len())
            .max()
            .unwrap_or(0);

        let mut combined_params = Vec::new();
        let mut min_required = 0;

        for i in 0..max_param_count {
            let mut param_types_at_pos = Vec::new();
            let mut any_required = false;

            for (params, _, has_rest) in &all_signatures {
                if i < params.len() {
                    let param = &params[i];
                    if param.rest {
                        // For rest params like `...b: number[]`, extract the element type
                        // so we intersect `number` (not `number[]`) with other members' types
                        if let Some(elem) = crate::type_queries::get_array_element_type(
                            self.interner,
                            param.type_id,
                        ) {
                            param_types_at_pos.push(elem);
                        } else {
                            // Can't extract element type; bail out
                            return None;
                        }
                    } else {
                        param_types_at_pos.push(param.type_id);
                    }
                    if param.is_required() {
                        any_required = true;
                    }
                } else if *has_rest {
                    // Position i is beyond this member's positional params, but the
                    // member has a rest param that covers all remaining positions.
                    // Include its element type in the intersection.
                    if let Some(rest_param) = params.last().filter(|p| p.rest)
                        && let Some(elem) = crate::type_queries::get_array_element_type(
                            self.interner,
                            rest_param.type_id,
                        )
                    {
                        param_types_at_pos.push(elem);
                    }
                }
                // If a member doesn't have a param at this position and has no rest,
                // it doesn't constrain the type (absent). But if ANY member requires
                // it, the combined signature requires it.
            }

            // Intersect all param types at this position
            let combined_type = if param_types_at_pos.len() == 1 {
                param_types_at_pos[0]
            } else if param_types_at_pos.is_empty() {
                // Shouldn't happen since we iterate up to max_param_count
                continue;
            } else {
                let mut result = param_types_at_pos[0];
                for &pt in &param_types_at_pos[1..] {
                    result = self.interner.intersection2(result, pt);
                }
                result
            };

            combined_params.push(combined_type);

            if any_required {
                min_required = i + 1;
            }
        }

        // Compute max_allowed using tsc's Phase 1 matching semantics:
        // The member(s) with the highest min_required become the "base" of the
        // combined signature (all other members' signatures partially match them
        // because their min ≤ base.min). The combined inherits the base member's
        // parameter shape for determining max_allowed.
        //
        // - If any base member has rest → unlimited (None)
        // - Otherwise → max of base members' param counts
        // - If all members have the same min, they're all base members → use
        //   existing max_param_count / any_has_rest logic
        let max_allowed = {
            // Compute per-member min_required
            let member_mins: Vec<usize> = all_signatures
                .iter()
                .map(|(params, _, _)| {
                    params
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.is_required() && !p.rest)
                        .map(|(i, _)| i + 1)
                        .max()
                        .unwrap_or(0)
                })
                .collect();

            let max_min = *member_mins.iter().max().unwrap_or(&0);

            // Collect base members (those with the highest min_required)
            let base_has_rest = all_signatures
                .iter()
                .zip(member_mins.iter())
                .any(|((_, _, has_rest), &m_min)| m_min == max_min && *has_rest);
            let base_max_params = all_signatures
                .iter()
                .zip(member_mins.iter())
                .filter(|&(_, &m_min)| m_min == max_min)
                .map(|((params, _, _), _)| params.len())
                .max()
                .unwrap_or(0);

            if base_has_rest {
                None // Base member(s) have rest → unlimited
            } else {
                Some(base_max_params)
            }
        };

        // Union all return types
        let return_types: Vec<TypeId> = all_signatures.iter().map(|(_, ret, _)| *ret).collect();
        let return_type = self.interner.union(return_types);

        Some(CombinedUnionSignature {
            param_types: combined_params,
            min_required,
            max_allowed,
            return_type,
        })
    }

    /// Compute the combined union signature for a union of construct signatures.
    ///
    /// Mirrors `try_compute_combined_union_signature` but uses `construct_signatures`
    /// instead of `call_signatures`. Returns `None` if any member has no (or multiple)
    /// construct signatures or if any is generic.
    pub(crate) fn try_compute_combined_union_construct_signature(
        &mut self,
        members: &[TypeId],
    ) -> Option<CombinedUnionSignature> {
        if members.is_empty() {
            return None;
        }

        // Collect single construct signatures from each member: (params, return_type, has_rest)
        let mut all_signatures: Vec<(Vec<ParamInfo>, TypeId, bool)> = Vec::new();

        for &member in members {
            let member = self.normalize_union_member(member);
            match self.interner.lookup(member) {
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    if callable.construct_signatures.len() != 1 {
                        return None; // 0 or multiple overloads — no combined
                    }
                    let sig = &callable.construct_signatures[0];
                    if !sig.type_params.is_empty() {
                        return None;
                    }
                    let params = self.normalize_union_signature_params(&sig.params);
                    let has_rest = params.iter().any(|p| p.rest);
                    all_signatures.push((params, sig.return_type, has_rest));
                }
                _ => return None, // not a constructable type with a single signature
            }
        }

        if all_signatures.is_empty() {
            return None;
        }

        let max_param_count = all_signatures
            .iter()
            .map(|(params, _, _)| params.len())
            .max()
            .unwrap_or(0);

        let mut combined_params = Vec::new();
        let mut min_required = 0;

        for i in 0..max_param_count {
            let mut param_types_at_pos = Vec::new();
            let mut any_required = false;

            for (params, _, has_rest) in &all_signatures {
                if i < params.len() {
                    let param = &params[i];
                    if param.rest {
                        if let Some(elem) = crate::type_queries::get_array_element_type(
                            self.interner,
                            param.type_id,
                        ) {
                            param_types_at_pos.push(elem);
                        } else {
                            return None;
                        }
                    } else {
                        // Strip `| undefined` that the binder may add for optional
                        // params (`b?: number` → type_id = `number | undefined`).
                        // The combined param type should be the raw type (`number`)
                        // so that error messages say "not assignable to 'number'"
                        // rather than "not assignable to 'number | undefined'".
                        let type_id = if param.optional {
                            crate::narrowing::utils::remove_undefined(self.interner, param.type_id)
                        } else {
                            param.type_id
                        };
                        param_types_at_pos.push(type_id);
                    }
                    if param.is_required() {
                        any_required = true;
                    }
                } else if *has_rest
                    && let Some(rest_param) = params.last().filter(|p| p.rest)
                    && let Some(elem) = crate::type_queries::get_array_element_type(
                        self.interner,
                        rest_param.type_id,
                    )
                {
                    param_types_at_pos.push(elem);
                }
            }

            let combined_type = if param_types_at_pos.len() == 1 {
                param_types_at_pos[0]
            } else if param_types_at_pos.is_empty() {
                continue;
            } else {
                let mut result = param_types_at_pos[0];
                for &pt in &param_types_at_pos[1..] {
                    result = self.interner.intersection2(result, pt);
                }
                result
            };

            combined_params.push(combined_type);

            if any_required {
                min_required = i + 1;
            }
        }

        let max_allowed = {
            let member_mins: Vec<usize> = all_signatures
                .iter()
                .map(|(params, _, _)| {
                    params
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.is_required() && !p.rest)
                        .map(|(i, _)| i + 1)
                        .max()
                        .unwrap_or(0)
                })
                .collect();

            let max_min = *member_mins.iter().max().unwrap_or(&0);

            let base_has_rest = all_signatures
                .iter()
                .zip(member_mins.iter())
                .any(|((_, _, has_rest), &m_min)| m_min == max_min && *has_rest);
            let base_max_params = all_signatures
                .iter()
                .zip(member_mins.iter())
                .filter(|&(_, &m_min)| m_min == max_min)
                .map(|((params, _, _), _)| params.len())
                .max()
                .unwrap_or(0);

            if base_has_rest {
                None
            } else {
                Some(base_max_params)
            }
        };

        let return_types: Vec<TypeId> = all_signatures.iter().map(|(_, ret, _)| *ret).collect();
        let return_type = self.interner.union(return_types);

        Some(CombinedUnionSignature {
            param_types: combined_params,
            min_required,
            max_allowed,
            return_type,
        })
    }

    fn build_union_call_result(
        &self,
        union_type: TypeId,
        failures: &mut Vec<CallResult>,
        return_types: Vec<TypeId>,
        combined_return_override: Option<TypeId>,
        force_not_callable_with_this_mismatch: bool,
    ) -> CallResult {
        if return_types.is_empty() {
            if failures.is_empty() {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }

            // At least one member failed with a non-NotCallable error
            // Check if all failures are ArgumentTypeMismatch - if so, compute the intersection
            // of all parameter types to get the expected type (e.g., for union of functions
            // with incompatible parameter types like (x: number) => void | (x: boolean) => void)
            let all_arg_mismatches = failures
                .iter()
                .all(|f| matches!(f, CallResult::ArgumentTypeMismatch { .. }));

            if all_arg_mismatches && !failures.is_empty() {
                // Extract all parameter types from the failures
                let mut param_types = Vec::new();
                for failure in failures.iter() {
                    if let CallResult::ArgumentTypeMismatch { expected, .. } = failure {
                        param_types.push(*expected);
                    }
                }

                // Compute the intersection of all parameter types
                // For incompatible primitives like number & boolean, this becomes never
                let intersected_param = if param_types.len() == 1 {
                    param_types[0]
                } else {
                    // Build intersection by combining all types
                    let mut result = param_types[0];
                    for &param_type in &param_types[1..] {
                        result = self.interner.intersection2(result, param_type);
                    }
                    result
                };

                // Return a single ArgumentTypeMismatch with the intersected type
                // Use the first argument type as the actual
                let actual_arg_type =
                    if let Some(CallResult::ArgumentTypeMismatch { actual, .. }) = failures.first()
                    {
                        *actual
                    } else {
                        // Should never reach here, but use ERROR instead of UNKNOWN
                        TypeId::ERROR
                    };

                // Use the combined return type from the union's signatures, but ONLY
                // when all union members expected the same parameter type. When params
                // differ (e.g., {x: string} vs {y: string}), excess property issues can
                // cause false failures, and leaking a non-ERROR return type would cascade
                // into downstream narrowing problems.
                let all_same_param = param_types.windows(2).all(|w| w[0] == w[1]);
                let combined_return = if all_same_param {
                    combined_return_override.unwrap_or(TypeId::ERROR)
                } else {
                    TypeId::ERROR
                };

                return CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: intersected_param,
                    actual: actual_arg_type,
                    fallback_return: combined_return,
                };
            }

            if force_not_callable_with_this_mismatch
                && failures
                    .iter()
                    .all(|f| matches!(f, CallResult::ThisTypeMismatch { .. }))
            {
                return match failures.first() {
                    Some(CallResult::ThisTypeMismatch {
                        expected_this,
                        actual_this,
                        ..
                    }) => CallResult::ThisTypeMismatch {
                        expected_this: *expected_this,
                        actual_this: *actual_this,
                        emit_not_callable: true,
                    },
                    _ => CallResult::NotCallable {
                        type_id: union_type,
                    },
                };
            }

            // Not all argument type mismatches, return the first failure
            return failures
                .drain(..)
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                });
        }

        if return_types.len() == 1 {
            return CallResult::Success(return_types[0]);
        }

        // Return a union of all return types
        let union_result = self.interner.union(return_types);
        CallResult::Success(union_result)
    }
    fn resolve_union_call(
        &mut self,
        union_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // Phase 0: Check `this` parameter for the union.
        // TSC computes the intersection of all members' `this` types and checks the
        // calling context against it. A call fails with TS2684 if the `this` context
        // doesn't satisfy ALL members' `this` requirements.
        // IMPORTANT: Defer `this` errors to after argument checking — TSC reports
        // argument errors (TS2345) before `this` context errors (TS2684).
        let mut deferred_this_error =
            if let Some(combined_this) = self.compute_union_this_type(&members) {
                let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
                if !self.checker.is_assignable_to(actual_this, combined_this) {
                    Some(CallResult::ThisTypeMismatch {
                        expected_this: combined_this,
                        actual_this,
                        emit_not_callable: false,
                    })
                } else {
                    None
                }
            } else {
                None
            };

        // Phase 0.5: Check multi-overload union members for compatible signatures.
        // When multiple union members have multiple overloads, first try to find
        // compatible signatures across members. If found, validate `this` types.
        // If not found, fall through to per-member resolution (Phase 2) which
        // resolves each member's overloads independently — this matches tsc's
        // behavior for cases like `(A[] | B[]).filter(cb)` where each array type
        // has overloaded `filter` but per-member resolution succeeds.
        let sig_lists = self.collect_union_call_signature_lists(&members);
        let has_multi_overload_members =
            sig_lists.iter().filter(|(_, sigs)| sigs.len() > 1).count();
        let mut force_not_callable_with_this_mismatch = false;
        let mut force_union_this_type = None;

        if has_multi_overload_members >= 2 {
            if let Some(unified_sigs) = self.find_union_compatible_signatures(&sig_lists) {
                // Compatible signatures found — check `this` type constraint.
                // The unified signatures have intersected `this` types from
                // the matched overloads across all members.
                let unified_this = unified_sigs
                    .iter()
                    .filter_map(|s| s.this_type)
                    .reduce(|a, b| self.interner.intersection2(a, b));

                if let Some(combined_this) = unified_this {
                    let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
                    if !self.checker.is_assignable_to(actual_this, combined_this) {
                        return CallResult::ThisTypeMismatch {
                            expected_this: combined_this,
                            actual_this,
                            emit_not_callable: false,
                        };
                    }
                }
            } else {
                // No compatible signatures found across multi-overload members.
                // Per tsc's getUnionSignatures: when multiple union members have
                // multiple overloads and no compatible pair exists, the union is
                // not callable (TS2349). However, our compatibility check skips
                // generic signatures, so only report NotCallable when all overloads
                // across multi-overload members are non-generic. For generic
                // overloads, fall through to per-member resolution.
                let all_non_generic = sig_lists
                    .iter()
                    .filter(|(_, sigs)| sigs.len() > 1)
                    .all(|(_, sigs)| sigs.iter().all(|s| s.type_params.is_empty()));
                if all_non_generic {
                    let mut this_types = Vec::new();
                    for (_, sigs) in &sig_lists {
                        for sig in sigs {
                            if let Some(this_type) = sig.this_type {
                                this_types.push(this_type);
                            }
                        }
                    }
                    if this_types.is_empty() {
                        return CallResult::NotCallable {
                            type_id: union_type,
                        };
                    }

                    let mut combined_this = this_types[0];
                    for &this_type in &this_types[1..] {
                        combined_this = self.interner.intersection2(combined_this, this_type);
                    }
                    force_union_this_type = Some(combined_this);

                    force_not_callable_with_this_mismatch = true;
                }
            }
        } else if has_multi_overload_members == 1 {
            // One member has multiple overloads, others have one each.
            // Per tsc's getUnionSignatures/intersectSignatureSets: each single-overload
            // member's signature must be compatible with at least one overload from the
            // multi-overload member. If any single-overload member has no compatible
            // match, the union is not callable (TS2349).
            //
            // Use TypeId equality for `this` types (safe, no side effects) plus the
            // None-matches-any rule from tsc's compareSignaturesIdentical.
            let multi_idx = sig_lists.iter().position(|(_, sigs)| sigs.len() > 1);
            if let Some(multi_idx) = multi_idx {
                let multi_sigs = &sig_lists[multi_idx].1;
                let mut all_compatible = true;
                for (idx, (_, sigs)) in sig_lists.iter().enumerate() {
                    if idx == multi_idx {
                        continue;
                    }
                    if let Some(single_sig) = sigs.first() {
                        // Check if this single-overload sig is compatible with ANY
                        // overload from the multi-overload member.
                        let has_match = multi_sigs.iter().any(|multi_sig| {
                            // Skip generic signatures
                            if !single_sig.type_params.is_empty()
                                || !multi_sig.type_params.is_empty()
                            {
                                return false;
                            }
                            // Check required param count
                            let s_req =
                                single_sig.params.iter().filter(|p| p.is_required()).count();
                            let m_req = multi_sig.params.iter().filter(|p| p.is_required()).count();
                            if s_req != m_req {
                                return false;
                            }
                            // Check param types
                            let min_total = single_sig.params.len().min(multi_sig.params.len());
                            for i in 0..min_total {
                                if single_sig.params[i].type_id != multi_sig.params[i].type_id {
                                    return false;
                                }
                            }
                            // Check this types (None matches any per tsc)
                            match (single_sig.this_type, multi_sig.this_type) {
                                (Some(a), Some(b)) => a == b,
                                _ => true,
                            }
                        });
                        if !has_match {
                            all_compatible = false;
                            break;
                        }
                    }
                }
                if !all_compatible {
                    let all_non_generic = sig_lists
                        .iter()
                        .filter(|(_, sigs)| sigs.len() > 1)
                        .all(|(_, sigs)| sigs.iter().all(|s| s.type_params.is_empty()));
                    if all_non_generic {
                        return CallResult::NotCallable {
                            type_id: union_type,
                        };
                    }
                }
            }
        }

        if deferred_this_error.is_none()
            && force_not_callable_with_this_mismatch
            && let Some(expected_this) = force_union_this_type
        {
            let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
            if !self.checker.is_assignable_to(actual_this, expected_this) {
                deferred_this_error = Some(CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this,
                    emit_not_callable: false,
                });
            }
        }

        // Try to compute a combined signature for the union.
        // TypeScript computes combined arity (max required params across members)
        // and intersected parameter types with unioned return types.
        let combined = self.try_compute_combined_union_signature(&members);

        // Phase 1: Argument count validation using combined signature.
        // This catches cases where members have different param counts —
        // the combined signature requires the maximum number of params.
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

        // Phase 2: Per-member resolution for argument type checking.
        // This avoids over-constraining via intersection when tsc would reduce the union.
        let compatibility = if combined.is_some() {
            // Combined signature already validated arity; skip old bounds check
            UnionCallSignatureCompatibility::Unknown
        } else {
            let compat = self.union_call_signature_bounds(&members);
            if matches!(compat, UnionCallSignatureCompatibility::Incompatible) {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }
            compat
        };

        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            let result = self.resolve_call(member, arg_types);
            match result {
                CallResult::Success(return_type) => {
                    return_types.push(return_type);
                }
                CallResult::ThisTypeMismatch { .. } => {
                    // Per-member `this` failures mean arguments were validated
                    // successfully — only the `this` context was wrong. In union
                    // context, `this` is checked at the union level (Phase 0
                    // deferred check), so treat this as argument-success and
                    // extract the member's return type.
                    if let Some(ret) = crate::type_queries::get_return_type(self.interner, member) {
                        return_types.push(ret);
                    } else {
                        failures.push(result);
                    }
                }
                CallResult::NotCallable { .. } => {
                    return CallResult::NotCallable {
                        type_id: union_type,
                    };
                }
                other => {
                    failures.push(other);
                }
            }
        }
        // Phase 3: Result aggregation.
        if let Some(ref combined) = combined
            && !arg_types.iter().any(|&arg_type| {
                crate::type_queries::contains_type_parameters_db(self.interner, arg_type)
            })
        {
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
        }

        // When we have a combined signature and some members fail on arity
        // (because they have fewer params than the combined requires),
        // use the combined return type since the overall call is valid.
        if let Some(ref combined) = combined {
            let all_failures_are_arity = !failures.is_empty()
                && failures
                    .iter()
                    .all(|f| matches!(f, CallResult::ArgumentCountMismatch { .. }));

            if all_failures_are_arity && !return_types.is_empty() {
                // Some members succeeded, some failed on arity only.
                // The combined arity check passed, so the call is valid.
                return CallResult::Success(combined.return_type);
            }

            if all_failures_are_arity && return_types.is_empty() {
                // All members failed on arity but combined check passed.
                // Validate argument types against the combined (intersected) params.
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
                return CallResult::Success(combined.return_type);
            }

            // When all per-member resolutions fail (with any combination of arity
            // or type mismatches), validate against the combined (intersected)
            // parameter types instead. TSC intersects parameter types across union
            // members, so an arg like `{x: 0, y: 0}` satisfies `{x: number} &
            // {y: number}` even though it fails excess-property checks against each
            // individual member type. Individual members may also fail on arity when
            // they have fewer params than the combined signature allows.
            if !failures.is_empty() && return_types.is_empty() {
                let mut all_pass = true;
                for (i, &arg_type) in arg_types.iter().enumerate() {
                    if i < combined.param_types.len() {
                        let param_type = combined.param_types[i];
                        if !self.checker.is_assignable_to(arg_type, param_type) {
                            all_pass = false;
                            break;
                        }
                    }
                }
                if all_pass {
                    return CallResult::Success(combined.return_type);
                }
                // When the combined (intersected) parameter type is `never` and all
                // per-member calls fail, this MAY be a false positive from correlated
                // type parameters. For example, calling a union of functions obtained
                // from `MappedType[K]` where the argument type correlates with K.
                // In tsc, K links the handler and argument, so the call succeeds.
                //
                // Only apply this fallback when the argument types contain type
                // parameters — this indicates a generic/correlated context where
                // the caller's type parameter determines which union member is
                // actually reached. When all arguments are fully concrete (e.g.,
                // `string | number` passed to `((a: string) => void) | ((a: number) => void)`),
                // tsc correctly rejects the call (TS2345).
                let has_generic_args = arg_types.iter().any(|&arg_type| {
                    crate::type_queries::contains_type_parameters_db(self.interner, arg_type)
                });
                if has_generic_args && combined.param_types.contains(&TypeId::NEVER) {
                    let all_arg_mismatch = failures
                        .iter()
                        .all(|f| matches!(f, CallResult::ArgumentTypeMismatch { .. }));
                    if all_arg_mismatch {
                        let mut param_union_pass = true;
                        for (i, &arg_type) in arg_types.iter().enumerate() {
                            if i < combined.param_types.len()
                                && combined.param_types[i] == TypeId::NEVER
                            {
                                // Collect per-member param types at this position
                                let member_param_types: Vec<TypeId> = failures
                                    .iter()
                                    .filter_map(|f| {
                                        if let CallResult::ArgumentTypeMismatch {
                                            expected, ..
                                        } = f
                                        {
                                            Some(*expected)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let param_union = self.interner.union(member_param_types);
                                if !self.checker.is_assignable_to(arg_type, param_union) {
                                    param_union_pass = false;
                                    break;
                                }
                            }
                        }
                        if param_union_pass {
                            return CallResult::Success(combined.return_type);
                        }
                    }
                }
            }
        }

        // Standard per-member result aggregation (no combined signature or mixed failures)
        if !return_types.is_empty() {
            if combined.is_none()
                && !failures.is_empty()
                && members
                    .iter()
                    .copied()
                    .all(|member| self.is_single_signature_callable_member(member))
            {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }

            match compatibility {
                UnionCallSignatureCompatibility::Compatible {
                    min_required,
                    max_allowed,
                } => {
                    if arg_types.len() < min_required {
                        return CallResult::ArgumentCountMismatch {
                            expected_min: min_required,
                            expected_max: max_allowed,
                            actual: arg_types.len(),
                        };
                    }
                    if let Some(max_allowed) = max_allowed
                        && arg_types.len() > max_allowed
                    {
                        return CallResult::ArgumentCountMismatch {
                            expected_min: min_required,
                            expected_max: Some(max_allowed),
                            actual: arg_types.len(),
                        };
                    }
                    if failures
                        .iter()
                        .all(|f| matches!(f, CallResult::ArgumentCountMismatch { .. }))
                    {
                        return self.build_union_call_result(
                            union_type,
                            &mut failures,
                            return_types,
                            combined.as_ref().map(|c| c.return_type),
                            false,
                        );
                    }
                }
                UnionCallSignatureCompatibility::Unknown => {}
                UnionCallSignatureCompatibility::Incompatible => unreachable!(),
            }

            // Arguments resolved successfully — now check deferred `this` error.
            if let Some(this_error) = deferred_this_error {
                return this_error;
            }
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            let union_result = self.interner.union(return_types);
            CallResult::Success(union_result)
        } else if !failures.is_empty() {
            // Argument errors take priority over `this` context errors (matches tsc).
            self.build_union_call_result(
                union_type,
                &mut failures,
                return_types,
                combined.as_ref().map(|c| c.return_type),
                force_not_callable_with_this_mismatch,
            )
        } else if let Some(this_error) = deferred_this_error {
            this_error
        } else {
            CallResult::NotCallable {
                type_id: union_type,
            }
        }
    }

    /// Resolve a call on an intersection type.
    ///
    /// This handles cases like:
    /// - `Function & { prop: number }` - intersection with callable member
    /// - Overloaded functions merged via intersection
    ///
    /// When at least one intersection member is callable, this delegates to that member.
    /// For intersections with multiple callable members, we use the first one.
    fn resolve_intersection_call(
        &mut self,
        intersection_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // For intersection types: if ANY member is callable, the intersection is callable
        // This is different from unions where ALL members must be callable
        // We try each member in order and use the first callable one
        for &member in members.iter() {
            let result = self.resolve_call(member, arg_types);
            match result {
                CallResult::Success(return_type) => {
                    // Found a callable member - use its return type
                    return CallResult::Success(return_type);
                }
                CallResult::NotCallable { .. } => {
                    // This member is not callable, try the next one
                    continue;
                }
                other => {
                    // Got a different error (argument mismatch, etc.)
                    // Return this error as it's likely the most relevant
                    return other;
                }
            }
        }

        // No members were callable
        CallResult::NotCallable {
            type_id: intersection_type,
        }
    }

    /// Resolve a call to a simple function type.
    pub(crate) fn resolve_function_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // Handle generic functions FIRST so uninstantiated this_types don't fail assignability
        if !func.type_params.is_empty() {
            return self.resolve_generic_call(func, arg_types);
        }

        // Check `this` context if specified by the function shape.
        // IMPORTANT: Defer `this` errors to after argument checking — TSC reports
        // argument errors (TS2345) before `this` context errors (TS2684).
        let deferred_this_error = if let Some(expected_this) = func.this_type {
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
        let mut failures = Vec::new();
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
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // Constraint violation is a "near match" - return the type
                    // for overload resolution (treat as success with error)
                    return CallResult::Success(return_type);
                }
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
        // Use the last overload signature's return type as the fallback (matching
        // tsc behavior). tsc uses the last declaration's return type for error
        // recovery, allowing downstream code to see the expected shape. For
        // example, `[].concat(...)` on `never[]` should still produce `never[]`,
        // not `never`, so that chained `.map()` resolves correctly.
        let fallback_return = callable
            .call_signatures
            .last()
            .map(|s| s.return_type)
            .unwrap_or(TypeId::NEVER);
        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(callable.clone()),
            arg_types: arg_types.to_vec(),
            failures,
            fallback_return,
        }
    }
}

pub fn infer_call_signature<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    sig: &CallSignature,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_call_signature(sig, arg_types)
}

pub fn infer_generic_function<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func: &FunctionShape,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_generic_function(func, arg_types)
}

pub fn resolve_call_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
) -> CallWithCheckerResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.set_actual_this_type(actual_this_type);
    let result = evaluator.resolve_call(func_type, arg_types);
    let predicate = evaluator.last_instantiated_predicate.take();
    let instantiated_params = evaluator.last_instantiated_params.take();
    (result, predicate, instantiated_params)
}

pub fn resolve_new_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
) -> CallResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.resolve_new(type_id, arg_types)
}

pub fn compute_contextual_types_with_compat_checker<'a, R, F>(
    interner: &'a dyn QueryDatabase,
    resolver: &'a R,
    shape: &FunctionShape,
    arg_types: &[TypeId],
    contextual_type: Option<TypeId>,
    configure_checker: F,
) -> TypeSubstitution
where
    R: crate::TypeResolver,
    F: FnOnce(&mut crate::CompatChecker<'a, R>),
{
    let mut checker = crate::CompatChecker::with_resolver(interner, resolver);
    configure_checker(&mut checker);

    let mut evaluator = CallEvaluator::new(interner, &mut checker);
    evaluator.set_contextual_type(contextual_type);
    evaluator.compute_contextual_types(shape, arg_types)
}

pub fn get_contextual_signature_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::CompatChecker>::get_contextual_signature(db, type_id)
}

pub fn get_contextual_signature_for_arity_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::CompatChecker>::get_contextual_signature_for_arity(
        db,
        type_id,
        Some(arg_count),
    )
}
