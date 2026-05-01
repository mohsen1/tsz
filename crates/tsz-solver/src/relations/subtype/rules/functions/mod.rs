//! Function and callable type subtype checking.
//!
//! This module handles subtyping for TypeScript's callable types:
//! - Function types: `(x: number) => void`
//! - Callable objects: `{ (x: number): void; name: string }`
//! - Constructor types: `new (x: number) => T`
//! - Call signatures and overloads
//! - Parameter compatibility (contravariant/bivariant)
//! - Return type compatibility (covariant)
//! - Type predicate compatibility
//! - `this` parameter handling

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::type_param_info;
use crate::types::{
    CallSignature, FunctionShape, InferencePriority, ParamInfo, TypeData, TypeId, TypeParamInfo,
    TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

/// Build a `TypeSubstitution` that maps each type parameter to its constraint
/// (or `unknown` if unconstrained). This corresponds to tsc's `getCanonicalSignature`
/// behavior — used when generic signatures need to be compared structurally after
/// erasing their type parameter identities.
pub(super) fn erase_type_params_to_constraints(type_params: &[TypeParamInfo]) -> TypeSubstitution {
    let mut sub = TypeSubstitution::new();
    for tp in type_params {
        sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
    }
    sub
}

/// Build a `TypeSubstitution` that maps each type parameter to `any`.
/// This matches tsc's `getErasedSignature` / `createTypeEraser` behavior, which
/// maps type parameters to `any` (NOT to their constraints). Used in the N×M
/// signature comparison path (`signaturesRelatedTo` with `erase = true`) where
/// multiple overloaded signatures are compared against a target.
pub(super) fn erase_type_params_to_any(type_params: &[TypeParamInfo]) -> TypeSubstitution {
    let mut sub = TypeSubstitution::new();
    for tp in type_params {
        sub.insert(tp.name, TypeId::ANY);
    }
    sub
}

/// Erase a call signature's type parameters to `any`, producing a non-generic
/// `FunctionShape`. Used by the N×M signature comparison path.
pub(super) fn erase_call_sig_to_any(
    sig: &CallSignature,
    interner: &dyn crate::TypeDatabase,
) -> FunctionShape {
    use crate::instantiation::instantiate::instantiate_type;
    if sig.type_params.is_empty() {
        return FunctionShape {
            type_params: Vec::new(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        };
    }
    let sub = erase_type_params_to_any(&sig.type_params);
    let erased_params: Vec<_> = sig
        .params
        .iter()
        .map(|p| ParamInfo {
            name: p.name,
            type_id: instantiate_type(interner, p.type_id, &sub),
            optional: p.optional,
            rest: p.rest,
        })
        .collect();
    FunctionShape {
        type_params: Vec::new(),
        params: erased_params,
        this_type: sig.this_type,
        return_type: instantiate_type(interner, sig.return_type, &sub),
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: sig.is_method,
    }
}

/// Erase a function shape's type parameters to `any`, producing a non-generic
/// `FunctionShape`. Used by the N×M signature comparison path.
pub(super) fn erase_fn_shape_to_any(
    f: &FunctionShape,
    interner: &dyn crate::TypeDatabase,
) -> FunctionShape {
    use crate::instantiation::instantiate::instantiate_type;
    if f.type_params.is_empty() {
        return f.clone();
    }
    let sub = erase_type_params_to_any(&f.type_params);
    let erased_params: Vec<_> = f
        .params
        .iter()
        .map(|p| ParamInfo {
            name: p.name,
            type_id: instantiate_type(interner, p.type_id, &sub),
            optional: p.optional,
            rest: p.rest,
        })
        .collect();
    FunctionShape {
        type_params: Vec::new(),
        params: erased_params,
        this_type: f.this_type,
        return_type: instantiate_type(interner, f.return_type, &sub),
        type_predicate: f.type_predicate,
        is_constructor: f.is_constructor,
        is_method: f.is_method,
    }
}

pub(super) fn resolve_contextual_source_inference_candidate(
    lower_bounds: &[TypeId],
    inferred: TypeId,
) -> TypeId {
    if lower_bounds.is_empty() {
        return inferred;
    }

    let mut distinct = Vec::new();
    for &bound in lower_bounds {
        if !distinct.contains(&bound) {
            distinct.push(bound);
        }
    }

    if distinct.len() <= 1 {
        inferred
    } else {
        distinct[0]
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(crate) fn type_param_appears_in_mapped_context(
        &self,
        type_id: TypeId,
        param_name: tsz_common::interner::Atom,
    ) -> bool {
        crate::visitor::collect_all_types(self.interner, type_id)
            .into_iter()
            .any(|candidate| match self.interner.lookup(candidate) {
                Some(TypeData::Mapped(mapped_id)) => {
                    let mapped = self.interner.get_mapped(mapped_id);
                    crate::visitor::contains_type_parameter_named(
                        self.interner,
                        mapped.constraint,
                        param_name,
                    ) || crate::visitor::contains_type_parameter_named(
                        self.interner,
                        mapped.template,
                        param_name,
                    ) || mapped.name_type.is_some_and(|name_type| {
                        crate::visitor::contains_type_parameter_named(
                            self.interner,
                            name_type,
                            param_name,
                        )
                    })
                }
                _ => false,
            })
    }

    pub(crate) fn has_conflicting_contextual_param_candidates(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let tracked_type_params: FxHashSet<_> =
            source.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return false;
        }

        let source_params_unpacked: Vec<ParamInfo> = source
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();
        let target_params_unpacked: Vec<ParamInfo> = target
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();

        let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
        let target_fixed_count = if target_has_rest {
            target_params_unpacked.len().saturating_sub(1)
        } else {
            target_params_unpacked.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params_unpacked.len().saturating_sub(1)
        } else {
            source_params_unpacked.len()
        };

        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        let mut contextual_candidates: FxHashMap<_, Vec<TypeId>> = FxHashMap::default();

        for i in 0..fixed_compare_count {
            let s_param = &source_params_unpacked[i];
            let t_param = &target_params_unpacked[i];

            let s_effective = if s_param.optional {
                self.interner.union2(s_param.type_id, TypeId::UNDEFINED)
            } else {
                s_param.type_id
            };
            let t_effective = if t_param.optional {
                self.interner.union2(t_param.type_id, TypeId::UNDEFINED)
            } else {
                t_param.type_id
            };

            if self.is_uninformative_contextual_inference_input(t_effective) {
                continue;
            }

            // Only consider type parameters that appear *naked* (directly as the
            // parameter type itself). When a type parameter is nested inside a
            // complex type like `Foo<K>` or `(ev: WindowEventMap[K]) => void`,
            // the target parameter type at that position is NOT a candidate for
            // K — it is the type for the whole parameter. Comparing these
            // unrelated target types causes false conflicts (e.g., `"message"`
            // vs `Action1<...>` when K appears in both `type: K` and
            // `listener: (ev: WindowEventMap[K]) => any`).
            if let Some(info) = type_param_info(self.interner, s_effective)
                && tracked_type_params.contains(&info.name)
            {
                contextual_candidates
                    .entry(info.name)
                    .or_default()
                    .push(t_effective);
            }
        }

        contextual_candidates.values().any(|candidates| {
            for (idx, &left) in candidates.iter().enumerate() {
                for &right in candidates.iter().skip(idx + 1) {
                    if left == right {
                        continue;
                    }
                    let comparable = self.check_subtype(left, right).is_true()
                        || self.check_subtype(right, left).is_true();
                    if !comparable {
                        return true;
                    }
                }
            }
            false
        })
    }

    /// Check if parameter types are compatible based on variance settings.
    ///
    /// In strict mode (contravariant): `target_type` <: `source_type`
    /// In legacy mode (bivariant): `target_type` <: `source_type` OR `source_type` <: `target_type`
    /// See <https://github.com/microsoft/TypeScript/issues/18654>.
    pub(crate) fn are_parameters_compatible(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        self.are_parameters_compatible_impl(source_type, target_type, false)
    }

    /// Check if type predicates in functions are compatible.
    ///
    /// Type predicates make functions more specific. A function with a type predicate
    /// can only be assigned to another function with a compatible predicate.
    ///
    /// Rules:
    /// - No predicate vs no predicate: compatible
    /// - Source has predicate, target doesn't: compatible (source is more specific)
    /// - Target has type guard, source doesn't: NOT compatible (caller expects narrowing)
    /// - Target has assertion predicate, source doesn't: compatible (assertion is call-site annotation)
    /// - Both have predicates: check if predicates are compatible
    pub(crate) fn are_type_predicates_compatible(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        match (&source.type_predicate, &target.type_predicate) {
            // No predicates in either function, or source has predicate
            // but target doesn't — compatible. A function with a type
            // predicate is callable where a plain boolean-returning
            // function is expected.
            (None, None) | (Some(_), None) => true,

            // Target has predicate, source doesn't.
            // For type guards (`x is T`, `this is T`): NOT compatible.
            // A plain boolean-returning function cannot satisfy a type
            // predicate contract (the caller expects narrowing).
            // For assertion predicates (`asserts x`, `asserts x is T`):
            // compatible — tsc allows assigning a plain void-returning
            // function to an assertion function slot. The assertion
            // predicate is a call-site narrowing annotation, not a
            // runtime contract that the implementation must satisfy.
            (None, Some(target_pred)) => target_pred.asserts,

            // Both have predicates — check compatibility
            (Some(source_pred), Some(target_pred)) => {
                // First, check if predicates target the same parameter.
                // We compare by parameter index if available, falling back to name
                // comparison only if indices are missing (e.g. for synthetic types).
                let targets_match = match (source_pred.parameter_index, target_pred.parameter_index)
                {
                    (Some(s_idx), Some(t_idx)) => s_idx == t_idx,
                    _ => source_pred.target == target_pred.target,
                };

                if !targets_match {
                    return false;
                }

                // Check asserts compatibility
                // Type guards (`x is T`) and assertions (`asserts x is T`) are NOT compatible
                match (source_pred.asserts, target_pred.asserts) {
                    // Source is type guard, target is assertion - NOT compatible
                    // Source is assertion, target is type guard - NOT compatible
                    (false, true) | (true, false) => false,
                    // Both same type - check type compatibility
                    (false, false) | (true, true) => {
                        match (source_pred.type_id, target_pred.type_id) {
                            (Some(source_type), Some(target_type)) => {
                                if source_type == target_type {
                                    return true;
                                }
                                // Evaluate to normalize Application/Intersection
                                // representations before comparison.
                                let se = self.evaluate_type(source_type);
                                let te = self.evaluate_type(target_type);
                                se == te || self.check_subtype(se, te).is_true()
                            }
                            (None, Some(_)) => false,
                            (Some(_), None) | (None, None) => true,
                        }
                    }
                }
            }
        }
    }

    /// Check parameter compatibility with method bivariance support.
    /// Methods are bivariant even when `strict_function_types` is enabled.
    pub(crate) fn are_parameters_compatible_impl(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        is_method: bool,
    ) -> bool {
        // Fast path: if types are identical, they're always compatible
        if source_type == target_type {
            return true;
        }

        // Fast path: `any` in either parameter position is always compatible
        // in permissive mode. In strict mode (TopLevelOnly), we require structural
        // compatibility unless both are ANY.
        // NOTE: North Star mandate #3.3 - any should not silence structural mismatches.
        if source_type.is_any() || target_type.is_any() {
            use crate::relations::subtype::AnyPropagationMode;
            if matches!(self.any_propagation, AnyPropagationMode::All) {
                return true;
            }
            if source_type == target_type {
                return true;
            }
            // Fall through to structural check for unsound any parameters
        }

        // Call-only and construct-only parameter types are not interchangeable.
        // Without this guard, constructor bivariance can incorrectly accept
        // higher-order mismatches by finding compatibility in one direction.
        let (s_has_call, s_has_construct) = self.callable_modality_flags_for_type(source_type);
        let (t_has_call, t_has_construct) = self.callable_modality_flags_for_type(target_type);
        let s_call_only = s_has_call && !s_has_construct;
        let s_construct_only = s_has_construct && !s_has_call;
        let t_call_only = t_has_call && !t_has_construct;
        let t_construct_only = t_has_construct && !t_has_call;
        if (s_call_only && t_construct_only) || (s_construct_only && t_call_only) {
            return false;
        }

        // Methods are bivariant regardless of strict_function_types setting
        // UNLESS disable_method_bivariance is set.
        // NOTE: North Star V1.2 prioritizes soundness. Bivariance is enabled for methods
        // even in strict mode to match modern TypeScript behavior.
        let method_should_be_bivariant = is_method && !self.disable_method_bivariance;
        let use_bivariance = method_should_be_bivariant || !self.strict_function_types;

        if !use_bivariance {
            // Contravariant check: Target <: Source
            // This applies even when parameter types contain `this` types.
            // The `this` type is polymorphic but does not change parameter variance.
            self.check_subtype(target_type, source_type).is_true()
        } else {
            // Bivariant: either direction works (Unsound, Legacy TS behavior)
            // Try contravariant first: Target <: Source
            if self.check_subtype(target_type, source_type).is_true() {
                return true;
            }
            // If contravariant fails, try covariant: Source <: Target
            self.check_subtype(source_type, target_type).is_true()
        }
    }

    /// Check if `this` parameters are compatible.
    ///
    /// TypeScript only checks `this` parameter compatibility when the target
    /// declares an explicit `this` parameter. If the target has no `this` parameter,
    /// any source `this` type is acceptable.
    pub(crate) fn are_this_parameters_compatible(
        &mut self,
        source_type: Option<TypeId>,
        target_type: Option<TypeId>,
    ) -> bool {
        // If target has no explicit `this` parameter, always compatible.
        // TypeScript only checks `this` when the target declares one.
        if target_type.is_none() {
            return true;
        }
        let source_type = source_type.unwrap_or(TypeId::UNKNOWN);
        let target_type = target_type.unwrap_or(TypeId::UNKNOWN);

        // this parameters follow the same variance rules as regular parameters
        if self.strict_function_types {
            // Contravariant in strict mode
            self.check_subtype(target_type, source_type).is_true()
        } else {
            // Bivariant in non-strict mode
            self.check_subtype(source_type, target_type).is_true()
                || self.check_subtype(target_type, source_type).is_true()
        }
    }

    /// Count required (non-optional, non-rest) parameters.
    pub(crate) fn required_param_count(&self, params: &[ParamInfo]) -> usize {
        crate::utils::required_param_count(params)
    }

    /// Compute effective parameter types for a pair of parameters being compared
    /// in signature compatibility.
    ///
    /// TypeScript compares declared optional parameter types during signature
    /// compatibility rather than eagerly widening them to `T | undefined`.
    /// That keeps `(x: string) => void` assignable to `(x?: string) => void`
    /// and vice versa, matching the solver unit tests and tsc's behavior for
    /// regular function signature relation checks.
    ///
    /// When both parameters are optional, strip `undefined` from their types
    /// so `(x?: T)` and `(x?: T | undefined)` compare as equivalent. This
    /// matches tsc's behavior where both forms are interchangeable in
    /// signature comparison.
    ///
    /// When only one parameter is optional (or neither), returns the raw types
    /// without stripping, preserving the stricter comparison needed to catch
    /// legitimate undefined-related mismatches.
    pub(crate) fn effective_param_type_pair(
        &self,
        s_param: &ParamInfo,
        t_param: &ParamInfo,
    ) -> (TypeId, TypeId) {
        if s_param.optional && t_param.optional {
            (
                self.strip_undefined_from_param_type(s_param.type_id),
                self.strip_undefined_from_param_type(t_param.type_id),
            )
        } else {
            (s_param.type_id, t_param.type_id)
        }
    }

    /// Strip `undefined` from a type for optional parameter normalization.
    /// If the type is `undefined` itself, returns `never`.
    /// If the type is a union containing `undefined`, returns the union without it.
    /// Otherwise returns the type as-is.
    fn strip_undefined_from_param_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::UNDEFINED {
            return TypeId::NEVER;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            if members.contains(&TypeId::UNDEFINED) {
                let filtered: Vec<TypeId> = members
                    .iter()
                    .copied()
                    .filter(|&m| m != TypeId::UNDEFINED)
                    .collect();
                if filtered.len() == 1 {
                    return filtered[0];
                }
                if filtered.len() > 1 {
                    return self.interner.union(filtered);
                }
                return TypeId::NEVER;
            }
        }
        type_id
    }

    /// Check if a parameter type contains `void` — either is `void` directly
    /// or is a union with `void` as a member (e.g., `number | void`).
    pub(crate) fn param_type_contains_void(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::VOID {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members.contains(&TypeId::VOID);
        }
        false
    }

    pub(crate) fn tuple_min_required_args(&self, elements: &[crate::TupleElement]) -> usize {
        elements
            .iter()
            .map(|elem| {
                if elem.rest {
                    let expansion = self.expand_tuple_rest(elem.type_id);
                    self.tuple_min_required_args(&expansion.fixed)
                        + self.tuple_min_required_args(&expansion.tail)
                } else if elem.optional || self.param_type_contains_void(elem.type_id) {
                    0
                } else {
                    1
                }
            })
            .sum()
    }

    pub(crate) fn rest_param_min_required_arg_count(&mut self, type_id: TypeId) -> usize {
        match self.interner.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                return self.rest_param_min_required_arg_count(inner);
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                return info
                    .constraint
                    .map(|constraint| self.rest_param_min_required_arg_count(constraint))
                    .unwrap_or(0);
            }
            _ => {}
        }

        let evaluated = self.evaluate_type(type_id);
        if evaluated != type_id {
            return self.rest_param_min_required_arg_count(evaluated);
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Tuple(elements_id)) => {
                let elements = self.interner.tuple_list(elements_id);
                self.tuple_min_required_args(&elements)
            }
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .map(|&member| self.rest_param_min_required_arg_count(member))
                    .min()
                    .unwrap_or(0)
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .map(|&member| self.rest_param_min_required_arg_count(member))
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    pub(crate) fn rest_param_needs_min_arity_guard(&mut self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                return self.rest_param_needs_min_arity_guard(inner);
            }
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => {
                return true;
            }
            _ => {}
        }

        let evaluated = self.evaluate_type(type_id);
        if evaluated != type_id {
            return self.rest_param_needs_min_arity_guard(evaluated);
        }

        match self.interner.lookup(type_id) {
            Some(
                TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::Mapped(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _),
            ) => true,
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .any(|&member| self.rest_param_needs_min_arity_guard(member)),
            _ => false,
        }
    }

    /// Check return type compatibility with void special-casing.
    ///
    /// When `allow_void_return` is true and target returns void:
    /// - Any source return type is acceptable (return value is ignored)
    /// - This enables `() => void` to accept functions with any return type
    pub(crate) fn check_return_compat(
        &mut self,
        source_return: TypeId,
        target_return: TypeId,
    ) -> SubtypeResult {
        if self.allow_void_return && target_return == TypeId::VOID {
            return SubtypeResult::True;
        }

        let source_needs_raw_fallback = matches!(
            self.interner.lookup(source_return),
            Some(TypeData::Application(_) | TypeData::Lazy(_))
        ) && self.evaluate_type(source_return) == TypeId::UNKNOWN;
        let target_needs_raw_fallback = matches!(
            self.interner.lookup(target_return),
            Some(TypeData::Application(_) | TypeData::Lazy(_))
        ) && self.evaluate_type(target_return) == TypeId::UNKNOWN;

        if source_needs_raw_fallback || target_needs_raw_fallback {
            let prev = self.bypass_evaluation;
            self.bypass_evaluation = true;
            let raw_result = self.check_subtype(source_return, target_return);
            self.bypass_evaluation = prev;
            if raw_result.is_false() {
                return raw_result;
            }
        }

        self.check_subtype(source_return, target_return)
    }

    pub(crate) fn instantiate_function_shape(
        &self,
        shape: &FunctionShape,
        substitution: &TypeSubstitution,
    ) -> FunctionShape {
        let params = shape
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let this_type = shape
            .this_type
            .map(|this_id| instantiate_type(self.interner, this_id, substitution));
        let return_type = instantiate_type(self.interner, shape.return_type, substitution);
        let type_predicate = shape.type_predicate.as_ref().map(|pred| TypePredicate {
            asserts: pred.asserts,
            target: pred.target,
            type_id: pred
                .type_id
                .map(|ty| instantiate_type(self.interner, ty, substitution)),
            parameter_index: pred.parameter_index,
        });

        FunctionShape {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        }
    }

    pub(crate) fn normalize_rest_param_types(&mut self, shape: &mut FunctionShape) {
        for param in &mut shape.params {
            if !param.rest {
                continue;
            }
            if matches!(
                self.interner.lookup(param.type_id),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ) {
                // Preserve bare type-parameter rest slots such as `...args: T`.
                // Eagerly evaluating them to their constraints (often `any[]`)
                // drops the min-arity guard used by function assignability and
                // incorrectly treats the rest as a top-like catch-all.
                continue;
            }
            let evaluated = self.evaluate_type(param.type_id);
            if evaluated != param.type_id {
                param.type_id = evaluated;
            }
        }
    }

    pub(crate) fn is_effective_never_type(&mut self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                self.is_effective_never_type(inner)
            }
            _ => {
                let evaluated = self.evaluate_type(type_id);
                evaluated == TypeId::NEVER
            }
        }
    }

    pub(crate) fn first_top_rest_unassignable_source_param(
        &mut self,
        params: &[ParamInfo],
    ) -> Option<(usize, TypeId)> {
        use crate::type_queries::unpack_tuple_rest_parameter;

        params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .enumerate()
            .find_map(|(index, param)| {
                if param.rest {
                    let elem_type = self.get_array_element_type(param.type_id);
                    self.is_effective_never_type(elem_type)
                        .then_some((index, elem_type))
                } else if !param.optional && self.is_effective_never_type(param.type_id) {
                    Some((index, param.type_id))
                } else {
                    None
                }
            })
    }

    const fn is_uninformative_contextual_inference_input(&self, ty: TypeId) -> bool {
        ty.is_any_unknown_or_error()
    }

    pub(crate) fn infer_source_type_param_substitution(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Result<TypeSubstitution, crate::inference::infer::InferenceError> {
        use crate::type_queries::unpack_tuple_rest_parameter;
        use std::fmt::Write;

        // Alpha-rename the source function's own type parameters before contextual
        // inference so outer target type parameters with the same names do not collide
        // in the inference context.
        let mut rename_substitution = TypeSubstitution::new();
        let mut renamed_type_params = Vec::with_capacity(source.type_params.len());
        let mut rename_buf = String::with_capacity(32);
        for (index, tp) in source.type_params.iter().enumerate() {
            rename_buf.clear();
            write!(rename_buf, "__infer_src_ctx_{index}").expect("write to String is infallible");
            let fresh_name = self.interner.intern_string(&rename_buf);
            let fresh_type = self.interner.type_param(TypeParamInfo {
                name: fresh_name,
                constraint: None,
                default: None,
                is_const: tp.is_const,
            });
            rename_substitution.insert(tp.name, fresh_type);
            renamed_type_params.push(TypeParamInfo {
                name: fresh_name,
                constraint: tp.constraint.map(|constraint| {
                    instantiate_type(self.interner, constraint, &rename_substitution)
                }),
                default: tp
                    .default
                    .map(|default| instantiate_type(self.interner, default, &rename_substitution)),
                is_const: tp.is_const,
            });
        }
        let renamed_source = FunctionShape {
            type_params: renamed_type_params,
            params: source
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &rename_substitution),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect(),
            this_type: source
                .this_type
                .map(|this_id| instantiate_type(self.interner, this_id, &rename_substitution)),
            return_type: instantiate_type(self.interner, source.return_type, &rename_substitution),
            type_predicate: source.type_predicate.as_ref().map(|pred| TypePredicate {
                asserts: pred.asserts,
                target: pred.target,
                type_id: pred
                    .type_id
                    .map(|ty| instantiate_type(self.interner, ty, &rename_substitution)),
                parameter_index: pred.parameter_index,
            }),
            is_constructor: source.is_constructor,
            is_method: source.is_method,
        };

        let mut infer_ctx = InferenceContext::new(self.interner);
        for tp in &renamed_source.type_params {
            let var = infer_ctx.fresh_type_param(tp.name, tp.is_const);
            if let Some(constraint) = tp.constraint {
                infer_ctx.add_upper_bound(var, constraint);
                infer_ctx.set_declared_constraint(var, constraint);
            }
        }

        let source_params_unpacked: Vec<ParamInfo> = renamed_source
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();
        let target_params_unpacked: Vec<ParamInfo> = target
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();

        let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params_unpacked
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let target_fixed_count = if target_has_rest {
            target_params_unpacked.len().saturating_sub(1)
        } else {
            target_params_unpacked.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params_unpacked.len().saturating_sub(1)
        } else {
            source_params_unpacked.len()
        };

        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source_params_unpacked[i];
            let t_param = &target_params_unpacked[i];

            let s_effective = if s_param.optional {
                self.interner.union2(s_param.type_id, TypeId::UNDEFINED)
            } else {
                s_param.type_id
            };
            let t_effective = if t_param.optional {
                self.interner.union2(t_param.type_id, TypeId::UNDEFINED)
            } else {
                t_param.type_id
            };

            if !self.is_uninformative_contextual_inference_input(t_effective) {
                let was_contra = infer_ctx.in_contra_mode;
                infer_ctx.in_contra_mode = true;
                let _ = infer_ctx.infer_from_types(
                    s_effective,
                    t_effective,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
            }
        }

        if target_has_rest
            && let Some(rest_elem_type) = rest_elem_type
            && !self.is_uninformative_contextual_inference_input(rest_elem_type)
        {
            for s_param in source_params_unpacked
                .iter()
                .take(source_fixed_count)
                .skip(target_fixed_count)
            {
                let was_contra = infer_ctx.in_contra_mode;
                infer_ctx.in_contra_mode = true;
                let _ = infer_ctx.infer_from_types(
                    s_param.type_id,
                    rest_elem_type,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
            }

            if source_has_rest && let Some(s_rest_param) = source_params_unpacked.last() {
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                let was_contra = infer_ctx.in_contra_mode;
                infer_ctx.in_contra_mode = true;
                let _ = infer_ctx.infer_from_types(
                    s_rest_elem,
                    rest_elem_type,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
            }
        }

        if source_has_rest && let Some(rest_param) = source_params_unpacked.last() {
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            for t_param in target_params_unpacked
                .iter()
                .take(target_fixed_count)
                .skip(source_fixed_count)
            {
                if !self.is_uninformative_contextual_inference_input(t_param.type_id) {
                    let was_contra = infer_ctx.in_contra_mode;
                    infer_ctx.in_contra_mode = true;
                    let _ = infer_ctx.infer_from_types(
                        rest_elem_type,
                        t_param.type_id,
                        InferencePriority::NakedTypeVariable,
                    );
                    infer_ctx.in_contra_mode = was_contra;
                }
            }
        }

        // When inferring a generic source signature in the context of a concrete
        // target signature, incompatible parameter-driven contra-candidates for
        // the same source type parameter must not be "repaired" by return-type
        // inference. In cases like `<T>(x: {a:T; b:T}) => T` contextualized by
        // `(x: {a:string; b:number}) => Object`, return inference can otherwise
        // push `T = Object` and incorrectly accept an unsound parameter relation.
        let mut conflicting_param_contra_candidates: FxHashSet<_> = FxHashSet::default();
        if target.type_params.is_empty() {
            for (original_tp, renamed_tp) in source
                .type_params
                .iter()
                .zip(renamed_source.type_params.iter())
            {
                let Some(var) = infer_ctx.find_type_param(renamed_tp.name) else {
                    continue;
                };
                let contra_candidates = infer_ctx.get_contra_candidate_types(var);

                let mut has_conflict = false;
                for i in 0..contra_candidates.len() {
                    for &right in contra_candidates.iter().skip(i + 1) {
                        let left = contra_candidates[i];
                        if left == right {
                            continue;
                        }
                        let comparable = self.check_subtype(left, right).is_true()
                            || self.check_subtype(right, left).is_true();
                        if !comparable {
                            has_conflict = true;
                            break;
                        }
                    }
                    if has_conflict {
                        break;
                    }
                }

                if has_conflict {
                    conflicting_param_contra_candidates.insert(original_tp.name);
                }
            }
        }

        if conflicting_param_contra_candidates.is_empty()
            && !self.is_uninformative_contextual_inference_input(target.return_type)
        {
            let _ = infer_ctx.infer_from_types(
                target.return_type,
                renamed_source.return_type,
                InferencePriority::ReturnType,
            );
        }
        if let (Some(source_this), Some(target_this)) = (renamed_source.this_type, target.this_type)
            && !self.is_uninformative_contextual_inference_input(target_this)
        {
            let _ = infer_ctx.infer_from_types(
                target_this,
                source_this,
                InferencePriority::NakedTypeVariable,
            );
        }
        if let (Some(source_pred), Some(target_pred)) =
            (&renamed_source.type_predicate, &target.type_predicate)
            && let (Some(source_ty), Some(target_ty)) = (source_pred.type_id, target_pred.type_id)
            && !self.is_uninformative_contextual_inference_input(target_ty)
        {
            let _ = infer_ctx.infer_from_types(target_ty, source_ty, InferencePriority::ReturnType);
        }

        // Try full inference first. If it fails (e.g., BoundsViolation when a
        // covariant return-type candidate conflicts with a contravariant parameter
        // upper bound), fall back to using parameter-based upper bounds directly.
        // This matches tsc's behavior where contextual signature instantiation
        // for subtype checking uses parameter inference over return-type inference.
        //
        // When a type param has a declared constraint and the inference fails
        // because actual inferred candidates violate the constraint, the caller
        // should fall back to constraint erasure (`getErasedSignature` in tsc).
        // However, when no actual inference happened (all target types were
        // uninformative like `unknown`), we should NOT fall back — the fallback
        // logic below will correctly default to `unknown`, matching tsc's
        // `getDefaultTypeArgumentType`.
        let inferred = infer_ctx.resolve_all_with_constraints();
        if let Err(e) = &inferred
            && source.type_params.iter().any(|tp| tp.constraint.is_some())
        {
            // Check if actual inference candidates were collected. If so, the
            // constraint violation is meaningful and we should fall back to
            // constraint erasure. If not (all inputs were uninformative), let
            // the fallback logic below handle it.
            let has_actual_candidates = source
                .type_params
                .iter()
                .zip(renamed_source.type_params.iter())
                .any(|(_, renamed_tp)| {
                    infer_ctx
                        .find_type_param(renamed_tp.name)
                        .is_some_and(|var| infer_ctx.var_has_candidates(var))
                });
            if has_actual_candidates {
                return Err(e.clone());
            }
        }
        let mut substitution = TypeSubstitution::new();
        for (original_tp, renamed_tp) in source
            .type_params
            .iter()
            .zip(renamed_source.type_params.iter())
        {
            let lower_bounds = infer_ctx
                .find_type_param(renamed_tp.name)
                .map(|var| {
                    infer_ctx
                        .get_constraints(var)
                        .map(|constraints| constraints.lower_bounds)
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let (upper_bounds, has_any_bounds) = infer_ctx
                .find_type_param(renamed_tp.name)
                .and_then(|var| infer_ctx.get_constraints(var))
                .map(|constraints| {
                    let has_any_bounds = !constraints.lower_bounds.is_empty()
                        || !constraints.upper_bounds.is_empty();
                    (constraints.upper_bounds, has_any_bounds)
                })
                .unwrap_or_default();
            let has_conflicting_param_upper_bounds =
                conflicting_param_contra_candidates.contains(&original_tp.name);
            let inferred_ty = inferred.as_ref().ok().and_then(|results| {
                results
                    .iter()
                    .find_map(|(name, ty)| (*name == renamed_tp.name).then_some(*ty))
            });
            // When inference collected no actual candidates (all inputs were
            // uninformative, e.g., `unknown` from a canonicalized target), the
            // resolver defaults to the declared constraint. But tsc's
            // `instantiateSignatureInContextOf` defaults to `unknown` when no
            // candidates exist (`getDefaultTypeArgumentType`). Detect this case
            // and use `unknown` instead, so that the subsequent structural
            // comparison doesn't fail due to contravariant parameter positions.
            let has_candidates = infer_ctx
                .find_type_param(renamed_tp.name)
                .is_some_and(|var| infer_ctx.var_has_candidates(var));
            let no_actual_inference_candidates = lower_bounds.is_empty()
                && !has_candidates
                && original_tp.constraint.is_some()
                && upper_bounds
                    .iter()
                    .all(|&ub| original_tp.constraint == Some(ub));
            let inferred_ty = if has_conflicting_param_upper_bounds {
                None
            } else if no_actual_inference_candidates
                && inferred_ty.is_some()
                && inferred_ty == original_tp.constraint
            {
                Some(TypeId::UNKNOWN)
            } else {
                inferred_ty
            };
            let fallback_ty = if has_conflicting_param_upper_bounds {
                None
            } else if inferred_ty.is_none() {
                // No inference result — try using parameter-based upper bounds.
                // When parameters provide a concrete type (e.g., T <: string from
                // a parameter position), use the tightest upper bound as the
                // inferred type. This handles cases like:
                //   <T>(x: T) => T  assigned to  (x: string) => Object
                // where T should resolve to string (from parameter) not Object
                // (from return type which caused BoundsViolation).
                let param_upper_bounds: Vec<TypeId> = infer_ctx
                    .find_type_param(renamed_tp.name)
                    .and_then(|var| infer_ctx.get_constraints(var))
                    .map(|cs| cs.upper_bounds)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|&ub| {
                        // Filter out declared constraints (already present on the
                        // type param) — we only want inferred upper bounds from
                        // parameter positions.
                        original_tp.constraint != Some(ub)
                    })
                    .collect();
                if param_upper_bounds.len() == 1 {
                    Some(param_upper_bounds[0])
                } else if param_upper_bounds.len() > 1 {
                    Some(self.interner.intersection(param_upper_bounds))
                } else {
                    None
                }
            } else {
                None
            };
            let inferred_is_unconstrained_unknown =
                inferred_ty == Some(TypeId::UNKNOWN) && !has_any_bounds && upper_bounds.is_empty();
            let preserve_uninferred_type_param = has_conflicting_param_upper_bounds
                || ((inferred_ty.is_none() || inferred_is_unconstrained_unknown)
                    && fallback_ty.is_none()
                    && original_tp.constraint.is_none()
                    && (source.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, original_tp.name)
                    }) || source.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, original_tp.name)
                    }) || self.type_param_appears_in_mapped_context(
                        source.return_type,
                        original_tp.name,
                    )));
            let fallback = if self.strict_function_types {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };
            let resolved_ty = inferred_ty
                .filter(|ty| !(*ty == TypeId::UNKNOWN && preserve_uninferred_type_param))
                .map(|ty| resolve_contextual_source_inference_candidate(&lower_bounds, ty))
                .or(fallback_ty);
            if let Some(resolved_ty) = resolved_ty {
                substitution.insert(original_tp.name, resolved_ty);
            } else if !preserve_uninferred_type_param {
                substitution.insert(original_tp.name, fallback);
            }
        }
        Ok(substitution)
    }
}

mod checking;
