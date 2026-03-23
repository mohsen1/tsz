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
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId,
    InferencePriority, ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, TypeData, TypeId,
    TypeParamInfo, TypePredicate, Visibility,
};
use crate::visitor::contains_this_type;
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

/// Build a `TypeSubstitution` that maps each type parameter to its constraint
/// (or `unknown` if unconstrained). This corresponds to tsc's `getErasedSignature` /
/// `getCanonicalSignature` behavior — used when generic signatures need to be
/// compared structurally after erasing their type parameter identities.
fn erase_type_params_to_constraints(type_params: &[TypeParamInfo]) -> TypeSubstitution {
    let mut sub = TypeSubstitution::new();
    for tp in type_params {
        sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
    }
    sub
}

fn resolve_contextual_source_inference_candidate(
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
    fn type_param_appears_in_mapped_context(
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

    fn has_conflicting_contextual_param_candidates(
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
    /// - Target has predicate, source doesn't: compatible (more lenient)
    /// - Both have predicates: check if predicates are compatible
    pub(crate) fn are_type_predicates_compatible(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        match (&source.type_predicate, &target.type_predicate) {
            // No predicates in either function - compatible
            (None, None) | (Some(_), None) | (None, Some(_)) => true,

            // Source has predicate, target doesn't - allow assignment.
            // Type predicates are implemented as runtime boolean returns, so a function with
            // a predicate is still callable where a plain boolean-returning function is
            // expected (as in ReturnType<T>).
            // Both have predicates - check compatibility
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

        let contains_this =
            self.type_contains_this_type(source_type) || self.type_contains_this_type(target_type);

        // Methods are bivariant regardless of strict_function_types setting
        // UNLESS disable_method_bivariance is set.
        // NOTE: North Star V1.2 prioritizes soundness. Bivariance is enabled for methods
        // even in strict mode to match modern TypeScript behavior.
        let method_should_be_bivariant = is_method && !self.disable_method_bivariance;
        let use_bivariance = method_should_be_bivariant || !self.strict_function_types;

        if !use_bivariance {
            if contains_this {
                return self.check_subtype(source_type, target_type).is_true();
            }
            // Contravariant check: Target <: Source
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

    /// Check if a type contains the `this` type anywhere in its structure.
    pub(crate) fn type_contains_this_type(&self, type_id: TypeId) -> bool {
        contains_this_type(self.interner, type_id)
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

    /// Check if a parameter type contains `void` — either is `void` directly
    /// or is a union with `void` as a member (e.g., `number | void`).
    fn param_type_contains_void(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::VOID {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members.contains(&TypeId::VOID);
        }
        false
    }

    fn tuple_min_required_args(&self, elements: &[crate::TupleElement]) -> usize {
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

    fn rest_param_needs_min_arity_guard(&mut self, type_id: TypeId) -> bool {
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

    fn instantiate_function_shape(
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
            target: pred.target.clone(),
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

    fn normalize_rest_param_types(&mut self, shape: &FunctionShape) -> FunctionShape {
        let mut normalized = shape.clone();
        for param in &mut normalized.params {
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
        normalized
    }

    fn is_effective_never_type(&mut self, type_id: TypeId) -> bool {
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
        matches!(ty, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
    }

    fn infer_source_type_param_substitution(
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
                target: pred.target.clone(),
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

        if !self.is_uninformative_contextual_inference_input(target.return_type) {
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
                        .map_or(false, |var| infer_ctx.var_has_candidates(var))
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
            let no_actual_inference_candidates = lower_bounds.is_empty()
                && original_tp.constraint.is_some()
                && upper_bounds
                    .iter()
                    .all(|&ub| original_tp.constraint == Some(ub));
            let inferred_ty = if no_actual_inference_candidates
                && inferred_ty.is_some()
                && inferred_ty == original_tp.constraint
            {
                Some(TypeId::UNKNOWN)
            } else {
                inferred_ty
            };
            let fallback_ty = if inferred_ty.is_none() {
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
            let preserve_uninferred_type_param = (inferred_ty.is_none()
                || inferred_is_unconstrained_unknown)
                && fallback_ty.is_none()
                && original_tp.constraint.is_none()
                && (source.params.iter().any(|param| {
                    self.type_param_appears_in_mapped_context(param.type_id, original_tp.name)
                }) || source.this_type.is_some_and(|this_type| {
                    self.type_param_appears_in_mapped_context(this_type, original_tp.name)
                }) || self
                    .type_param_appears_in_mapped_context(source.return_type, original_tp.name));
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

    pub(crate) fn check_function_subtype(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        // Constructor vs non-constructor
        if source.is_constructor != target.is_constructor {
            return SubtypeResult::False;
        }

        let mut source_instantiated = source.clone();
        let mut target_instantiated = target.clone();
        // Track type param equivalences scope for cleanup at end of function.
        let equiv_start = self.type_param_equivalences.len();

        // Generic source vs generic target (same arity): normalize both signatures so they
        // can be compared structurally.
        //
        // Two strategies are used depending on constraint compatibility:
        // 1. Alpha-renaming: map target type params to source type params, check constraints
        //    bidirectionally. Works when constraints are related (especially outer-scope type
        //    parameters like `T` vs `T1 extends T`).
        // 2. Canonicalization (tsc-like): replace target type params with their constraints,
        //    then infer source type params from the concrete target. Handles cases where
        //    constraints differ structurally but are semantically equivalent through parameter
        //    usage (e.g., `<S extends {p:string}[]>(x: S)` vs `<T extends {p:string}>(x: T[])`).
        if !source_instantiated.type_params.is_empty()
            && source_instantiated.type_params.len() == target_instantiated.type_params.len()
            && !target_instantiated.type_params.is_empty()
        {
            let mut target_to_source_substitution = TypeSubstitution::new();
            let mut source_identity_substitution = TypeSubstitution::new();
            for (source_tp, target_tp) in source_instantiated
                .type_params
                .iter()
                .zip(target_instantiated.type_params.iter())
            {
                let source_type_param_type = self.interner.type_param(*source_tp);
                target_to_source_substitution.insert(target_tp.name, source_type_param_type);
                source_identity_substitution.insert(source_tp.name, source_type_param_type);
            }

            let mapped_constraint_sensitive =
                source_instantiated.type_params.iter().any(|tp| {
                    source_instantiated.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || source_instantiated.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self.type_param_appears_in_mapped_context(
                        source_instantiated.return_type,
                        tp.name,
                    ) || target_instantiated.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || target_instantiated.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self.type_param_appears_in_mapped_context(
                        target_instantiated.return_type,
                        tp.name,
                    )
                });

            // Mapped/indexed generic signatures are constraint-sensitive: a stricter
            // target constraint like `U extends string[]` must stay visible rather
            // than being alpha-renamed onto an unconstrained source parameter `T`,
            // or apparent-member facts can be erased and make the signatures look
            // spuriously compatible. Outside that lane, keep the broader one-way
            // compatibility that TypeScript uses for generic function directionality.
            let constraints_allow_alpha_rename = source_instantiated
                .type_params
                .iter()
                .zip(target_instantiated.type_params.iter())
                .all(|(source_tp, target_tp)| {
                    let source_constraint = source_tp.constraint.unwrap_or(TypeId::UNKNOWN);
                    let target_constraint =
                        target_tp.constraint.map_or(TypeId::UNKNOWN, |constraint| {
                            instantiate_type(
                                self.interner,
                                constraint,
                                &target_to_source_substitution,
                            )
                        });

                    let target_to_source = self
                        .check_subtype(target_constraint, source_constraint)
                        .is_true();
                    let source_to_target = self
                        .check_subtype(source_constraint, target_constraint)
                        .is_true();

                    if mapped_constraint_sensitive {
                        // Both directions must hold for mapped/indexed contexts
                        // to preserve constraint information
                        target_to_source && source_to_target
                    } else {
                        // Match tsc's typeParametersRelatedTo: allow alpha-rename
                        // if constraints are compatible in EITHER direction.
                        // e.g., <T>(a: T) => T  vs  <T extends Derived>(a: T) => T
                        // target_constraint=unknown, source_constraint=Derived:
                        // unknown ≤ Derived fails, but Derived ≤ unknown succeeds.
                        target_to_source || source_to_target
                    }
                });

            if constraints_allow_alpha_rename {
                // Strategy 1: alpha-rename — both shapes use source type param identities.
                //
                // Establish type parameter equivalences for structural comparison.
                // When return types are pre-evaluated Object types (e.g., IList<D> already
                // expanded to an Object shape), name-based substitution may fail to penetrate
                // inner functions with same-named type params (shadowing). The equivalences
                // allow structural comparison to treat the original source/target type params
                // as identical, fixing false mismatches for structurally identical generic
                // method signatures with different type param names.
                for (source_tp, target_tp) in source_instantiated
                    .type_params
                    .iter()
                    .zip(target_instantiated.type_params.iter())
                {
                    let source_tp_type = self.interner.type_param(*source_tp);
                    let target_tp_type = self.interner.type_param(*target_tp);
                    if source_tp_type != target_tp_type {
                        self.type_param_equivalences
                            .push((source_tp_type, target_tp_type));
                    }
                }

                source_instantiated = self.instantiate_function_shape(
                    &source_instantiated,
                    &source_identity_substitution,
                );
                target_instantiated = self.instantiate_function_shape(
                    &target_instantiated,
                    &target_to_source_substitution,
                );
            } else {
                // Strategy 2: canonicalize target — replace type params with constraints,
                // then fall through to the generic-source → non-generic-target inference path
                let canonical_substitution =
                    erase_type_params_to_constraints(&target_instantiated.type_params);
                target_instantiated =
                    self.instantiate_function_shape(&target_instantiated, &canonical_substitution);
                target_instantiated.type_params.clear();
            }
        }

        // When both sides are generic but have different type parameter counts,
        // erase both signatures by replacing type params with their constraints
        // (or `unknown` if unconstrained). This matches tsc's `getCanonicalSignature`
        // behavior in `signatureRelatedTo` when `eraseGenerics` is true.
        // Example: `<T, U>(x: T, y: U) => void` vs `<T>(x: T, y: T) => void`
        //   → erased: `(x: unknown, y: unknown) => void` vs `(x: unknown, y: unknown) => void`
        if !source_instantiated.type_params.is_empty()
            && !target_instantiated.type_params.is_empty()
            && source_instantiated.type_params.len() != target_instantiated.type_params.len()
        {
            let source_canonical =
                erase_type_params_to_constraints(&source_instantiated.type_params);
            source_instantiated =
                self.instantiate_function_shape(&source_instantiated, &source_canonical);

            let target_canonical =
                erase_type_params_to_constraints(&target_instantiated.type_params);
            target_instantiated =
                self.instantiate_function_shape(&target_instantiated, &target_canonical);
        }

        // Contextual signature instantiation for generic source -> non-generic target.
        // This is key for non-strict assignability cases where a generic function expression
        // is contextually typed by a concrete callback/function type.
        //
        // Two strategies exist and we try inference first (needed for contextual callback
        // typing where return types must be precisely inferred), then fall back to tsc's
        // `getErasedSignature` (constraint erasure) if the inference-based comparison fails.
        // This fallback is essential for interface-extends checks (TS2430) where inference
        // over-constrains by intersecting inferred types with constraints.
        let mut used_inference_for_generic_source = false;
        let source_before_generic_instantiation = if !source_instantiated.type_params.is_empty()
            && target_instantiated.type_params.is_empty()
        {
            Some(source_instantiated.clone())
        } else {
            None
        };
        if !source_instantiated.type_params.is_empty() && target_instantiated.type_params.is_empty()
        {
            // When a generic callback is inferred as an argument (e.g., `fn(function<T>(a: Foo<T>) {})`),
            // the outer function's type parameter (e.g., `Args`) gets inferred as a tuple containing
            // the callback's own type parameter TypeIds (e.g., `[Foo<T>, T]`). The target signature
            // is then instantiated with these inferred types, making it non-generic but containing
            // the source's type parameter TypeIds. In this case, the source and target already share
            // the same type parameter identity — no erasure or inference is needed; just clear the
            // source type params so structural comparison proceeds with matching TypeIds.
            let source_tp_ids: Vec<TypeId> = source_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let target_refs_source_params = target_instantiated.params.iter().any(|p| {
                source_tp_ids.contains(&p.type_id)
                    || source_tp_ids.iter().any(|&tp_id| {
                        crate::visitor::collect_all_types(self.interner, p.type_id).contains(&tp_id)
                    })
            }) || source_tp_ids.iter().any(|&tp_id| {
                crate::visitor::collect_all_types(self.interner, target_instantiated.return_type)
                    .contains(&tp_id)
            });

            if target_refs_source_params {
                // Target references source's type params — they share identity.
                // Just clear source type params; no instantiation needed.
                source_instantiated.type_params.clear();
            } else {
                if self.has_conflicting_contextual_param_candidates(
                    &source_instantiated,
                    &target_instantiated,
                ) {
                    return SubtypeResult::False;
                }
                let substitution = match self.infer_source_type_param_substitution(
                    &source_instantiated,
                    &target_instantiated,
                ) {
                    Ok(sub) => {
                        used_inference_for_generic_source = true;
                        sub
                    }
                    Err(_) => {
                        // Inference failed (e.g., bounds violation). Fall back to tsc's
                        // `getErasedSignature` behavior: replace type params with their
                        // constraints (or `unknown` if unconstrained).
                        erase_type_params_to_constraints(&source_instantiated.type_params)
                    }
                };
                source_instantiated =
                    self.instantiate_function_shape(&source_instantiated, &substitution);
            }
        }

        // Non-generic source → generic target: check if the source references the same
        // TypeParam TypeIds as the target's bound type parameters. This happens when
        // contextual type seeding resolves inference variables to the contextual type's
        // bound TypeParams (e.g., `wrap(list)` produces `(a: A) => A[]` where A is the
        // same TypeParam as in the contextual type `<A>(x: A) => A[]`).
        // In this case, treat the source as effectively generic with the same type params.
        // Otherwise, fall back to erasing target type params to constraints.
        if source_instantiated.type_params.is_empty() && !target_instantiated.type_params.is_empty()
        {
            let target_tp_ids: Vec<TypeId> = target_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let source_refs_target_params = source_instantiated.params.iter().any(|p| {
                target_tp_ids.contains(&p.type_id)
                    || target_tp_ids.iter().any(|&tp_id| {
                        crate::visitor::collect_all_types(self.interner, p.type_id).contains(&tp_id)
                    })
            }) || target_tp_ids.iter().any(|&tp_id| {
                crate::visitor::collect_all_types(self.interner, source_instantiated.return_type)
                    .contains(&tp_id)
            });

            if source_refs_target_params {
                // Source references target's bound TypeParams — promote source to generic
                // and use the same-arity alpha-renaming path above
                source_instantiated.type_params = target_instantiated.type_params.clone();
                // Both now have the same type params with the same TypeIds, so
                // alpha-renaming is an identity operation and structural comparison
                // will match correctly.
                target_instantiated.type_params.clear();
                source_instantiated.type_params.clear();
            } else if !self.erase_generics {
                // When erase_generics is false (strict mode, used for implements/extends
                // member type checking), a non-generic function is NOT assignable to a
                // generic function. This matches tsc's compareSignaturesRelated with
                // eraseGenerics=false: the comparison proceeds with raw TypeParameter
                // types in the target, and the SubtypeChecker rejects concrete types
                // against opaque type parameters (e.g., string ≤ T returns False).
                // This ensures TS2416 is correctly emitted for incompatible overrides.
                target_instantiated.type_params.clear();
            } else {
                // Default: erase target type params to their constraints so non-generic
                // functions can match generic targets structurally (e.g., for comparable
                // relation, overload resolution, and general type compatibility).
                let target_canonical =
                    erase_type_params_to_constraints(&target_instantiated.type_params);
                target_instantiated =
                    self.instantiate_function_shape(&target_instantiated, &target_canonical);
            }
        }

        source_instantiated = self.normalize_rest_param_types(&source_instantiated);
        target_instantiated = self.normalize_rest_param_types(&target_instantiated);

        // Return type is covariant
        let return_result = self.check_return_compat(
            source_instantiated.return_type,
            target_instantiated.return_type,
        );
        if !return_result.is_true() {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(
            source_instantiated.this_type,
            target_instantiated.this_type,
        ) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Type predicates check
        if !self.are_type_predicates_compatible(&source_instantiated, &target_instantiated) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Method/constructor bivariance: strictFunctionTypes only applies to function
        // type literals, not to methods or construct signatures (new (...) => T).
        let is_method = source_instantiated.is_method
            || target_instantiated.is_method
            || source_instantiated.is_constructor
            || target_instantiated.is_constructor;

        // The lib iterator/generator declarations encode `next(value?)` as a single
        // rest parameter with tuple-list type `[] | [TNext]`. Compare that whole
        // tuple-list type directly before the generic rest-element machinery kicks in;
        // otherwise we lose the contravariant relation between the tuple variants and
        // incorrectly accept incompatible `TNext` values.
        if let (Some(s_param), Some(t_param)) = (
            source_instantiated.params.first(),
            target_instantiated.params.first(),
        ) && source_instantiated.params.len() == 1
            && target_instantiated.params.len() == 1
            && s_param.rest
            && t_param.rest
            && self.is_tuple_list_rest_type(s_param.type_id)
            && self.is_tuple_list_rest_type(t_param.type_id)
        {
            self.type_param_equivalences.truncate(equiv_start);
            return if self.are_parameters_compatible_impl(
                s_param.type_id,
                t_param.type_id,
                is_method,
            ) {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // Unpack tuple rest parameters before comparison.
        // In TypeScript, `(...args: [A, B]) => R` is equivalent to `(a: A, b: B) => R`.
        // We unpack tuple rest parameters into individual fixed parameters for proper matching.
        // Before unpacking, evaluate Application types in rest params (e.g., MappedType<T>
        // that evaluates to a tuple) so unpack_tuple_rest_parameter can detect the tuple.
        use crate::type_queries::unpack_tuple_rest_parameter;
        let source_params_unpacked: Vec<ParamInfo> = source_instantiated
            .params
            .iter()
            .flat_map(|p| {
                if p.rest
                    && matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Application(_))
                    )
                {
                    let evaluated = self.evaluate_type(p.type_id);
                    if evaluated != p.type_id {
                        let mut ep = p.clone();
                        ep.type_id = evaluated;
                        return unpack_tuple_rest_parameter(self.interner, &ep);
                    }
                }
                unpack_tuple_rest_parameter(self.interner, p)
            })
            .collect();
        let target_params_unpacked: Vec<ParamInfo> = target_instantiated
            .params
            .iter()
            .flat_map(|p| {
                if p.rest
                    && matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Application(_))
                    )
                {
                    let evaluated = self.evaluate_type(p.type_id);
                    if evaluated != p.type_id {
                        let mut ep = p.clone();
                        ep.type_id = evaluated;
                        return unpack_tuple_rest_parameter(self.interner, &ep);
                    }
                }
                unpack_tuple_rest_parameter(self.interner, p)
            })
            .collect();

        if source_params_unpacked.len() == target_params_unpacked.len()
            && source_params_unpacked
                .iter()
                .zip(target_params_unpacked.iter())
                .all(|(source_param, target_param)| {
                    source_param.type_id == target_param.type_id
                        && source_param.optional == target_param.optional
                        && source_param.rest == target_param.rest
                })
        {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::True;
        }

        // Handle union-of-tuple rest parameters in target.
        // When target has `...args: [A] | [B, C] | [D]`, try each union member separately.
        // Source matches if its params are compatible with ANY of the union member tuple shapes.
        // This handles patterns like:
        //   interface I { set(...args: [Record<string, unknown>] | [string, unknown]): void }
        //   class C implements I { set(option: Record<string, unknown>): void; set(name: string, value: unknown): void; }
        if let Some(last_target_param) = target_instantiated.params.last()
            && last_target_param.rest
        {
            use crate::type_queries::data::get_union_members;
            if let Some(union_members) = get_union_members(self.interner, last_target_param.type_id)
            {
                // Get non-rest prefix params from target
                let prefix_count = target_params_unpacked.len().saturating_sub(1);
                let prefix_params: &[ParamInfo] = &target_params_unpacked[..prefix_count];

                let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
                for member_type_id in &union_members {
                    // When the union member is a readonly tuple and the source has
                    // individual (non-rest) parameters (forming a mutable tuple),
                    // the readonly tuple cannot be assigned to the mutable param tuple
                    // under contravariance.  Skip this member — it cannot match.
                    // This mirrors tsc's behavior where `readonly [A, B]` is not
                    // assignable to `[A, B]`.
                    if !source_has_rest
                        && matches!(
                            self.interner.lookup(*member_type_id),
                            Some(TypeData::ReadonlyType(_))
                        )
                    {
                        continue;
                    }

                    // Try unpacking this union member as a tuple
                    let member_param = ParamInfo {
                        type_id: *member_type_id,
                        rest: true,
                        ..last_target_param.clone()
                    };
                    let member_unpacked = unpack_tuple_rest_parameter(self.interner, &member_param);

                    // Build full param list for this variant
                    let mut variant_params: Vec<ParamInfo> = prefix_params.to_vec();
                    variant_params.extend(member_unpacked);

                    // Try the comparison with this variant
                    if self
                        .check_params_compatible(
                            &source_params_unpacked,
                            &variant_params,
                            is_method,
                        )
                        .is_true()
                    {
                        self.type_param_equivalences.truncate(equiv_start);
                        return SubtypeResult::True;
                    }
                }
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }
        }

        // Check rest parameter handling (after unpacking)
        let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params_unpacked
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        // Count non-rest parameters (needed for arity check below)
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

        // Check parameter arity: source's required params must not exceed
        // the target's total non-rest params (including optional ones).
        // When target has a rest parameter, skip the arity check entirely —
        // the rest parameter can accept any number of arguments, and type
        // compatibility of extra params is checked later against the rest element type.
        //
        // Special case: parameters of type `void` are effectively optional in TypeScript.
        // A function like `(a: void) => void` is assignable to `() => void` because
        // void parameters can be called without arguments.
        let source_required = self.required_param_count(&source_params_unpacked);
        let target_rest_min_required = if target_has_rest {
            target_params_unpacked
                .last()
                .map(|param| self.rest_param_min_required_arg_count(param.type_id))
                .unwrap_or(0)
        } else {
            0
        };
        let guard_target_rest_arity = target_has_rest
            && target_params_unpacked
                .last()
                .is_some_and(|param| self.rest_param_needs_min_arity_guard(param.type_id));
        if ((!self.allow_bivariant_param_count && !target_has_rest) || guard_target_rest_arity)
            && source_required
                > target_fixed_count
                    + if target_has_rest {
                        target_rest_min_required
                    } else {
                        0
                    }
        {
            let extra_are_void = source_params_unpacked
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count + target_rest_min_required))
                .all(|param| self.param_type_contains_void(param.type_id));
            if !extra_are_void {
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }
        }

        // Check parameter types
        let result = (|| -> SubtypeResult {
            // Compare fixed parameters (using unpacked params)
            let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
            for i in 0..fixed_compare_count {
                let s_param = &source_params_unpacked[i];
                let t_param = &target_params_unpacked[i];

                // Use declared parameter types directly for comparison.
                // ParamInfo.type_id stores the declared type (e.g., `number`
                // for `x?: number`), matching tsc's `getTypeAtPosition` which
                // does NOT include `| undefined` for optional params.
                // Optionality only affects arity counting, not type comparison.
                if !self.are_parameters_compatible_impl(s_param.type_id, t_param.type_id, is_method)
                {
                    // Trace: Parameter type mismatch
                    if let Some(tracer) = &mut self.tracer
                        && !tracer.on_mismatch_dyn(
                            crate::diagnostics::SubtypeFailureReason::ParameterTypeMismatch {
                                param_index: i,
                                source_param: s_param.type_id,
                                target_param: t_param.type_id,
                            },
                        )
                    {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::False;
                }
            }

            // If target has rest parameter, check source's extra params against the rest type
            if target_has_rest {
                let Some(rest_elem_type) = rest_elem_type else {
                    return SubtypeResult::False;
                };
                if rest_is_top {
                    if self
                        .first_top_rest_unassignable_source_param(&source_params_unpacked)
                        .is_some()
                    {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::True;
                }

                for s_param in source_params_unpacked
                    .iter()
                    .skip(target_fixed_count)
                    .take(source_fixed_count.saturating_sub(target_fixed_count))
                {
                    if !self.are_parameters_compatible_impl(
                        s_param.type_id,
                        rest_elem_type,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }

                if source_has_rest {
                    let Some(s_rest_param) = source_params_unpacked.last() else {
                        return SubtypeResult::False;
                    };

                    // After unpacking, tuple rest parameters are already expanded into fixed params.
                    // Only non-tuple rest parameters (like ...args: string[]) remain as rest.
                    // Check the rest element type against target's rest element type.
                    let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                    if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method)
                    {
                        return SubtypeResult::False;
                    }
                }
            }

            if source_has_rest {
                let rest_param = if let Some(rest_param) = source_params_unpacked.last() {
                    rest_param
                } else {
                    return SubtypeResult::False;
                };
                let rest_elem_type = self.get_array_element_type(rest_param.type_id);
                let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

                if !rest_is_top {
                    for t_param in target_params_unpacked
                        .iter()
                        .skip(source_fixed_count)
                        .take(target_fixed_count.saturating_sub(source_fixed_count))
                    {
                        if !self.are_parameters_compatible_impl(
                            rest_elem_type,
                            t_param.type_id,
                            is_method,
                        ) {
                            return SubtypeResult::False;
                        }
                    }
                }
            }

            SubtypeResult::True
        })();

        // If the inference-based comparison failed and we used inference for the
        // generic source → non-generic target case, retry with constraint erasure.
        // This matches tsc's `getErasedSignature` behavior for interface extension
        // checks (TS2430) where inference over-constrains type parameters by
        // intersecting inferred types with their constraints.
        let source_before_has_mapped_type_param_context = source_before_generic_instantiation
            .as_ref()
            .is_some_and(|source_before| {
                source_before.type_params.iter().any(|tp| {
                    source_before.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || source_before.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self
                        .type_param_appears_in_mapped_context(source_before.return_type, tp.name)
                })
            });
        if !result.is_true()
            && used_inference_for_generic_source
            && !source_before_has_mapped_type_param_context
            && let Some(source_before) = source_before_generic_instantiation
        {
            let erasure_sub = erase_type_params_to_constraints(&source_before.type_params);
            let erased_source = self.instantiate_function_shape(&source_before, &erasure_sub);
            let retry = self.check_function_subtype(&erased_source, &target_instantiated);
            self.type_param_equivalences.truncate(equiv_start);
            return retry;
        }

        // Clean up type parameter equivalences established in this scope.
        self.type_param_equivalences.truncate(equiv_start);
        result
    }

    fn is_tuple_list_rest_type(&mut self, type_id: TypeId) -> bool {
        use crate::type_queries::{get_tuple_elements, union_contains_tuple};

        get_tuple_elements(self.interner, type_id).is_some()
            || union_contains_tuple(self.interner, type_id)
    }

    /// Check if a single function type is a subtype of a callable type with overloads.
    pub(crate) fn check_function_to_callable_subtype(
        &mut self,
        s_fn_id: FunctionShapeId,
        t_callable_id: CallableShapeId,
    ) -> SubtypeResult {
        let s_fn = self.interner.function_shape(s_fn_id);
        let t_callable = self.interner.callable_shape(t_callable_id);

        for t_sig in &t_callable.call_signatures {
            if s_fn.is_constructor {
                return SubtypeResult::False;
            }
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                return SubtypeResult::False;
            }
        }

        for t_sig in &t_callable.construct_signatures {
            if !s_fn.is_constructor {
                return SubtypeResult::False;
            }
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                return SubtypeResult::False;
            }
        }

        // Check properties: a plain function has no user-defined properties,
        // so if the target callable has non-optional properties (e.g., from a
        // namespace merge), the function is NOT a subtype. This matches tsc's
        // behavior where `typeof Point` (function + namespace exports) is not
        // assignable to a bare function type.
        let should_skip_prop = |name: crate::intern::Atom| {
            let resolved = self.interner.resolve_atom(name);
            resolved.starts_with('#')
        };
        let target_props: Vec<_> = t_callable
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        if !target_props.is_empty() {
            // The function type has no properties to match against the target's
            // required properties. Delegate to check_object_subtype with an
            // empty source shape to properly handle optional vs required props.
            let source_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
            };
            let target_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: target_props,
                string_index: t_callable.string_index.clone(),
                number_index: t_callable.number_index.clone(),
                symbol: t_callable.symbol,
            };
            if !self
                .check_object_subtype(&source_shape, None, None, &target_shape, None)
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if an overloaded callable type is a subtype of a single function type.
    pub(crate) fn check_callable_to_function_subtype(
        &mut self,
        s_callable_id: CallableShapeId,
        t_fn_id: FunctionShapeId,
    ) -> SubtypeResult {
        let s_callable = self.interner.callable_shape(s_callable_id);
        let t_fn = self.interner.function_shape(t_fn_id);

        if t_fn.is_constructor {
            for s_sig in &s_callable.construct_signatures {
                if self
                    .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true()
                {
                    return SubtypeResult::True;
                }
            }
            return SubtypeResult::False;
        }

        if s_callable.call_signatures.is_empty() {
            return SubtypeResult::False;
        }

        // Check ALL source call signatures against the target function,
        // matching tsc's signaturesRelatedTo behavior where any compatible
        // source signature suffices (not just the last/implementation one).
        for s_sig in &s_callable.call_signatures {
            if self
                .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                .is_true()
            {
                return SubtypeResult::True;
            }

            // Try generic instantiation if the source sig has type params
            // but the target doesn't
            if !s_sig.type_params.is_empty()
                && t_fn.type_params.is_empty()
                && self
                    .try_instantiate_generic_callable_to_function(s_sig, &t_fn)
                    .is_true()
            {
                return SubtypeResult::True;
            }
        }
        SubtypeResult::False
    }

    /// Try to instantiate a generic callable signature to match a concrete function type.
    /// This handles cases like: `declare function box<V>(x: V): {value: V}; const f: (x: number) => {value: number} = box;`
    fn try_instantiate_generic_callable_to_function(
        &mut self,
        s_sig: &crate::types::CallSignature,
        t_fn: &crate::types::FunctionShape,
    ) -> SubtypeResult {
        use crate::TypeData;
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

        // Create a substitution mapping type parameters to the target's parameter types
        // This is a simplified instantiation - we map each source type param to the corresponding target param type
        let mut substitution = TypeSubstitution::new();

        // For a simple case like <V>(x: V) => R vs (x: T) => S, map V to T
        // This handles the common case where type parameters flow through from parameters to return type
        for (s_param, t_param) in s_sig.params.iter().zip(t_fn.params.iter()) {
            // If source param is a type parameter, map it to target param type
            if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(s_param.type_id) {
                substitution.insert(tp.name, t_param.type_id);
            }
        }

        // If we couldn't infer any type parameters, fall back to checking with unknown
        // This handles cases where type params aren't directly in parameters
        if substitution.is_empty() {
            for tp in &s_sig.type_params {
                substitution.insert(tp.name, crate::TypeId::UNKNOWN);
            }
        }

        // Instantiate the source signature
        let instantiated_params: Vec<_> = s_sig
            .params
            .iter()
            .map(|p| crate::types::ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        let instantiated_return = instantiate_type(self.interner, s_sig.return_type, &substitution);

        let instantiated_sig = crate::types::CallSignature {
            type_params: Vec::new(), // No type params after instantiation
            params: instantiated_params,
            this_type: s_sig.this_type,
            return_type: instantiated_return,
            type_predicate: s_sig.type_predicate.clone(),
            is_method: s_sig.is_method,
        };

        // Check if instantiated signature is compatible with target
        self.check_call_signature_subtype_to_fn(&instantiated_sig, t_fn)
    }

    /// Check callable subtyping with overloaded signatures.
    pub(crate) fn check_callable_subtype(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> SubtypeResult {
        // For each target call signature, at least one source call signature must match.
        // Unlike call-site overload resolution (which uses only the implementation/last
        // signature), structural subtype checking uses ALL source signatures — matching
        // tsc's signaturesRelatedTo N×M comparison.
        for t_sig in &target.call_signatures {
            let mut found_match = false;
            for s_sig in &source.call_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // For each target construct signature, at least one source signature must match.
        // Construct signatures use bivariant parameter checking (like methods).
        for t_sig in &target.construct_signatures {
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                let result = self.check_call_signature_subtype_as_constructor(s_sig, t_sig);
                if result.is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // Check properties (if any), excluding private fields.
        // Sort by name (Atom) to match the merge scan's expectation in check_object_subtype.
        //
        // When both callables have construct signatures (class constructors), skip the
        // `prototype` property. Its type is the instance type which is already validated
        // by construct signature compatibility — checking it separately can fail when
        // the target has generic type params that were erased only at the signature level.
        let has_construct_sigs =
            !source.construct_signatures.is_empty() && !target.construct_signatures.is_empty();
        let should_skip_prop = |name| {
            let resolved = self.interner.resolve_atom(name);
            resolved.starts_with('#') || (has_construct_sigs && resolved == "prototype")
        };
        let mut source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        // Function-like sources (with call signatures) are expected to have Function members
        // such as `call` and `apply`, even if those properties are not materialized on the
        // callable shape. Add synthetic members to align assignability behavior.
        if !source.call_signatures.is_empty() {
            for t_prop in &target.properties {
                let prop_name = self.interner.resolve_atom(t_prop.name);
                if (prop_name == "call" || prop_name == "apply")
                    && !source_props.iter().any(|p| p.name == t_prop.name)
                {
                    source_props.push(PropertyInfo {
                        name: t_prop.name,
                        type_id: t_prop.type_id,
                        write_type: t_prop.write_type,
                        optional: false,
                        readonly: false,
                        is_method: true,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                    });
                }
            }
        }
        source_props.sort_by_key(|a| a.name);
        let mut target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        target_props.sort_by_key(|a| a.name);
        // Create temporary ObjectShape instances for the property check
        let source_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: source_props,
            string_index: source.string_index.clone(),
            number_index: source.number_index.clone(),
            symbol: source.symbol,
        };
        let target_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: target_props,
            string_index: target.string_index.clone(),
            number_index: target.number_index.clone(),
            symbol: target.symbol,
        };
        if !self
            .check_object_subtype(&source_shape, None, None, &target_shape, None)
            .is_true()
        {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check call signature subtyping.
    pub(crate) fn check_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, false)
    }

    /// Check construct signature subtyping (bivariant parameters).
    pub(crate) fn check_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, true)
    }

    fn check_call_signature_subtype_impl(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        is_constructor: bool,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate.clone(),
            is_constructor,
            is_method: source.is_method,
        };
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate.clone(),
            is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype(&source_fn, &target_fn)
    }

    /// Check call signature subtype to function shape.
    pub(crate) fn check_call_signature_subtype_to_fn(
        &mut self,
        source: &CallSignature,
        target: &FunctionShape,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate.clone(),
            is_constructor: target.is_constructor,
            is_method: source.is_method,
        };
        self.check_function_subtype(&source_fn, target)
    }

    /// Check function shape subtype to call signature.
    pub(crate) fn check_call_signature_subtype_fn(
        &mut self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> SubtypeResult {
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate.clone(),
            is_constructor: source.is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype(source, &target_fn)
    }

    /// Check if source params are compatible with target params.
    /// Extracted to support union-of-tuple rest parameter handling,
    /// where we need to try multiple target param variants.
    fn check_params_compatible(
        &mut self,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
        is_method: bool,
    ) -> SubtypeResult {
        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        let target_fixed_count = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params.len().saturating_sub(1)
        } else {
            source_params.len()
        };

        let source_required = self.required_param_count(source_params);
        let target_rest_min_required = if target_has_rest {
            target_params
                .last()
                .map(|param| self.rest_param_min_required_arg_count(param.type_id))
                .unwrap_or(0)
        } else {
            0
        };
        let guard_target_rest_arity = target_has_rest
            && target_params
                .last()
                .is_some_and(|param| self.rest_param_needs_min_arity_guard(param.type_id));
        if ((!self.allow_bivariant_param_count && !target_has_rest) || guard_target_rest_arity)
            && source_required
                > target_fixed_count
                    + if target_has_rest {
                        target_rest_min_required
                    } else {
                        0
                    }
        {
            let extra_are_void = source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count + target_rest_min_required))
                .all(|param| self.param_type_contains_void(param.type_id));
            if !extra_are_void {
                return SubtypeResult::False;
            }
        }

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source_params[i];
            let t_param = &target_params[i];

            // Use declared types directly — see comment in check_function_params_subtype.
            if !self.are_parameters_compatible_impl(s_param.type_id, t_param.type_id, is_method) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False;
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for s_param in source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_fixed_count.saturating_sub(target_fixed_count))
            {
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source_params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source_params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

            if !rest_is_top {
                for t_param in target_params
                    .iter()
                    .skip(source_fixed_count)
                    .take(target_fixed_count.saturating_sub(source_fixed_count))
                {
                    if !self.are_parameters_compatible_impl(
                        rest_elem_type,
                        t_param.type_id,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }

    /// Evaluate a meta-type (conditional, index access, mapped, keyof, etc.) to its
    /// concrete form. Uses `TypeEvaluator` with the resolver to correctly resolve
    /// Lazy(DefId) types at all nesting levels (e.g., KeyOf(Lazy(DefId))).
    ///
    /// Always uses `TypeEvaluator` with the resolver instead of `query_db.evaluate_type()`
    /// because the checker populates DefId→TypeId mappings in the `TypeEnvironment` that
    /// the `query_db`'s resolver-less evaluator cannot access.
    ///
    /// Results are cached in `eval_cache` to avoid re-evaluating the same type across
    /// multiple subtype checks. This turns O(n²) evaluate calls into O(n).
    pub(crate) fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        // Check local evaluation cache first.
        // Key includes no_unchecked_indexed_access since with that flag evaluation results can vary.
        let cache_key = (type_id, self.no_unchecked_indexed_access);
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        use crate::evaluation::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        // Pass query_db to share the application evaluation cache across evaluations.
        // This ensures that Application(Lazy(DefId), args) evaluated multiple times produces
        // the same ObjectShapeId, preventing spurious structural subtype failures when two
        // independent evaluations of the same generic type (e.g., AsyncGenerator<string, string, string[]>)
        // produce different shape IDs.
        if let Some(db) = self.query_db {
            evaluator = evaluator.with_query_db(db);
        }
        let result = evaluator.evaluate(type_id);
        self.eval_cache.insert(cache_key, result);
        result
    }
}
