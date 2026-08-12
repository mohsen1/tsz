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
    FunctionShape, InferencePriority, ParamInfo, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

#[derive(Clone, Copy)]
struct CallbackParameterPair {
    source_nonnull: TypeId,
    target_nonnull: TypeId,
    enters_callback_mode: bool,
}

mod erasure;
use erasure::{erase_call_sig_to_any, erase_fn_shape_to_any, erase_type_params_to_constraints};

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
    pub(crate) fn with_provisional_rest_union_function_scope<T>(
        &mut self,
        operation: impl FnOnce(&mut Self, bool) -> T,
    ) -> T {
        let previous_depth = self.provisional_rest_union_function_depth;
        let allow_at_this_depth = self.allow_provisional_rest_union && previous_depth == 0;
        self.provisional_rest_union_function_depth = previous_depth.saturating_add(1);
        let result = operation(self, allow_at_this_depth);
        self.provisional_rest_union_function_depth = previous_depth;
        result
    }

    pub(crate) fn type_param_appears_in_mapped_context(
        &self,
        type_id: TypeId,
        param: TypeParamInfo,
    ) -> bool {
        crate::visitors::visitor_predicates::mapped_context_references_type_param_binder(
            self.interner,
            type_id,
            param,
        )
    }

    pub(crate) fn has_conflicting_contextual_param_candidates(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        use crate::type_queries::unpack_tuple_rest_parameter;

        if source.type_params.is_empty() {
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
                && source
                    .type_params
                    .iter()
                    .any(|type_param| type_param.is_same_binder(info))
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
                                if se == te || self.check_subtype(se, te).is_true() {
                                    return true;
                                }
                                if let Some(target_elem) = self
                                    .readonly_array_syntax_element(target_type)
                                    .or_else(|| self.readonly_array_syntax_element(te))
                                    && let Some(source_elem) =
                                        self.predicate_array_like_element_type(source_type, se)
                                {
                                    return self.check_subtype(source_elem, target_elem).is_true();
                                }
                                false
                            }
                            (None, Some(_)) => false,
                            (Some(_), None) | (None, None) => true,
                        }
                    }
                }
            }
        }
    }

    fn predicate_array_like_element_type(&self, raw: TypeId, evaluated: TypeId) -> Option<TypeId> {
        crate::type_queries::get_array_element_type(self.interner, raw)
            .or_else(|| crate::type_queries::get_array_element_type(self.interner, evaluated))
            .or_else(|| {
                crate::objects::IndexSignatureResolver::with_resolver(self.interner, self.resolver)
                    .resolve_number_index(raw)
            })
            .or_else(|| {
                (evaluated != raw).then(|| {
                    crate::objects::IndexSignatureResolver::with_resolver(
                        self.interner,
                        self.resolver,
                    )
                    .resolve_number_index(evaluated)
                })?
            })
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
        if self.same_named_type_param_application_pair(source_type, target_type, 0) {
            return true;
        }

        // `never` opposite an `any` parameter is the one pair the permissive
        // shortcut below must not answer. `tsc`'s `isSimpleTypeRelatedTo`
        // rejects `any -> never` outright (`if (t & TypeFlags.Never) return
        // false`) before any `any` allowance applies, so under
        // `strictFunctionTypes` the contravariant parameter check
        // `target <: source` rejects `(u: never) => R` against
        // `(u: any) => R2`, while the reverse pair stays compatible through
        // `never <: any`. Both answers already fall out of the ordinary
        // variance-directed check below (the same one that gets
        // `ReadonlyArray<any>` vs `ReadonlyArray<never>` right), so skip the
        // shortcut and let the direction decide instead of encoding it here.
        let never_opposite_any = (source_type.is_any() && target_type == TypeId::NEVER)
            || (target_type.is_any() && source_type == TypeId::NEVER);

        // Fast path: `any` in either parameter position is always compatible
        // in permissive mode. In strict mode (TopLevelOnly), we require structural
        // compatibility unless both are ANY.
        // NOTE: North Star mandate #3.3 - any should not silence structural mismatches.
        if !never_opposite_any && (source_type.is_any() || target_type.is_any()) {
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
        let force_strict_callback_params = self.force_strict_callback_param_variance;
        let use_bivariance = !force_strict_callback_params
            && (method_should_be_bivariant || !self.strict_function_types);

        let callback_pair = self.classify_callback_parameter_pair(
            source_type,
            target_type,
            s_has_call,
            t_has_call,
            method_should_be_bivariant,
        );
        let entering_callback_check = callback_pair.is_some_and(|pair| pair.enters_callback_mode);
        let entering_bivariant_callback_return =
            entering_callback_check && method_should_be_bivariant;
        let saved_in_callback = self.in_callback_param_check;
        let saved_in_bivariant_callback_return = self.in_bivariant_callback_return_check;
        if entering_callback_check {
            self.in_callback_param_check = true;
            self.in_bivariant_callback_return_check = entering_bivariant_callback_return;
        }

        let result = if !use_bivariance {
            // Contravariant check: Target <: Source
            // This applies even when parameter types contain `this` types.
            // The `this` type is polymorphic but does not change parameter
            // variance. Clear the immediate-callback strictness while recursing:
            // nested function comparisons reached through compareTypes start
            // fresh in tsc.
            self.check_subtype_from_parameter_compare(target_type, source_type)
                .is_true()
        } else {
            // Bivariant: either direction works (Unsound, Legacy TS behavior)
            // Try contravariant first: Target <: Source
            if self
                .check_subtype_from_parameter_compare(target_type, source_type)
                .is_true()
            {
                self.in_callback_param_check = saved_in_callback;
                self.in_bivariant_callback_return_check = saved_in_bivariant_callback_return;
                return true;
            }
            if entering_callback_check {
                self.in_callback_param_check = saved_in_callback;
                self.in_bivariant_callback_return_check = saved_in_bivariant_callback_return;
                return false;
            }
            // The first `check_subtype` consumed `in_callback_param_check`
            // inside `check_function_subtype_impl` (it captures and resets the
            // flag at function entry). Restore it so the covariant retry sees
            // the same callback-mode state as the bivariant attempt; otherwise
            // the inner method-bivariance loosening would silently re-enable
            // and accept assignments that strict-callback should reject.
            if entering_callback_check {
                self.in_callback_param_check = true;
            }
            // If contravariant fails, try covariant: Source <: Target
            self.check_subtype_from_parameter_compare(source_type, target_type)
                .is_true()
        };

        self.in_callback_param_check = saved_in_callback;
        self.in_bivariant_callback_return_check = saved_in_bivariant_callback_return;
        result
    }

    /// Classify a pair of callable parameter slots using the same nullability,
    /// instantiated-generic, and method-origin rules in both the direct
    /// relation and the contextual-retry guard.
    fn classify_callback_parameter_pair(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        source_has_call: bool,
        target_has_call: bool,
        method_should_be_bivariant: bool,
    ) -> Option<CallbackParameterPair> {
        // tsc probes `getSingleCallSignature(getNonNullableType(t))` and only
        // enters callback mode when both sides carry the same nullish facts.
        let source_nonnull = crate::narrowing::utils::remove_nullish(self.interner, source_type);
        let target_nonnull = crate::narrowing::utils::remove_nullish(self.interner, target_type);
        let source_is_nullable = source_nonnull != source_type;
        let target_is_nullable = target_nonnull != target_type;
        if source_is_nullable != target_is_nullable {
            return None;
        }
        let source_call_for_callback = if source_is_nullable {
            self.callable_modality_flags_for_type(source_nonnull).0
        } else {
            source_has_call
        };
        let target_call_for_callback = if target_is_nullable {
            self.callable_modality_flags_for_type(target_nonnull).0
        } else {
            target_has_call
        };
        if !source_call_for_callback || !target_call_for_callback {
            return None;
        }

        // Slots materialized from a generic method argument retain ordinary
        // method bivariance. They remain callable pairs, but do not enter the
        // immediate strict-callback mode.
        let originated_from_instantiated_generic =
            self.callback_param_originated_from_instantiated_generic(source_type, target_type);
        let enters_callback_mode = !originated_from_instantiated_generic
            && (method_should_be_bivariant
                || self.callable_first_signature_is_method(source_nonnull)
                || self.callable_first_signature_is_method(target_nonnull));
        Some(CallbackParameterPair {
            source_nonnull,
            target_nonnull,
            enters_callback_mode,
        })
    }

    fn same_named_type_param_application_pair(
        &self,
        source_type: TypeId,
        target_type: TypeId,
        depth: u8,
    ) -> bool {
        if source_type == target_type {
            return true;
        }
        if depth >= 16 {
            return false;
        }
        if let (Some(source_param), Some(target_param)) = (
            type_param_info(self.interner, source_type),
            type_param_info(self.interner, target_type),
        ) {
            return source_param.is_same_binder(target_param);
        }
        if let (Some(source_app_id), Some(target_app_id)) = (
            crate::visitor::application_id(self.interner, source_type),
            crate::visitor::application_id(self.interner, target_type),
        ) {
            let source_app = self.interner.type_application(source_app_id);
            let target_app = self.interner.type_application(target_app_id);
            return source_app.args.len() == target_app.args.len()
                && self.same_named_type_param_application_pair(
                    source_app.base,
                    target_app.base,
                    depth + 1,
                )
                && source_app.args.iter().zip(target_app.args.iter()).all(
                    |(&source_arg, &target_arg)| {
                        self.same_named_type_param_application_pair(
                            source_arg,
                            target_arg,
                            depth + 1,
                        )
                    },
                );
        }
        if let (Some((source_obj, source_key)), Some((target_obj, target_key))) = (
            crate::visitor::index_access_parts(self.interner, source_type),
            crate::visitor::index_access_parts(self.interner, target_type),
        ) {
            return self.same_named_type_param_application_pair(source_obj, target_obj, depth + 1)
                && self.same_named_type_param_application_pair(source_key, target_key, depth + 1);
        }
        if let (Some(source_inner), Some(target_inner)) = (
            crate::visitor::keyof_inner_type(self.interner, source_type),
            crate::visitor::keyof_inner_type(self.interner, target_type),
        ) {
            return self.same_named_type_param_application_pair(
                source_inner,
                target_inner,
                depth + 1,
            );
        }
        false
    }

    /// Returns true when the type is callable and the first call signature is
    /// method-flavored. Callback mode still applies for ordinary higher-order
    /// function parameters when the callback type itself came from a bivariant
    /// method signature, such as `{ foo(x: I): O }["foo"]`.
    fn callable_first_signature_is_method(&mut self, type_id: TypeId) -> bool {
        if self.callable_first_signature_is_method_direct(type_id) {
            return true;
        }
        let evaluated = self.evaluate_type(type_id);
        evaluated != type_id && self.callable_first_signature_is_method_direct(evaluated)
    }

    fn callable_first_signature_is_method_direct(&self, type_id: TypeId) -> bool {
        if let Some(shape_id) = crate::visitor::function_shape_id(self.interner, type_id) {
            return self.interner.function_shape(shape_id).is_method;
        }
        if let Some(shape_id) = crate::visitor::callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            if let Some(sig) = shape.call_signatures.first() {
                return sig.is_method;
            }
        }
        false
    }

    fn check_subtype_from_parameter_compare(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> SubtypeResult {
        let saved_force_strict = self.force_strict_callback_param_variance;
        self.force_strict_callback_param_variance = false;
        let result = self.check_subtype(source_type, target_type);
        self.force_strict_callback_param_variance = saved_force_strict;
        result
    }

    fn callback_param_originated_from_instantiated_generic(
        &self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        self.instantiated_generic_method_args
            .iter()
            .any(|&arg| self.type_contains_instantiated_generic_arg(source_type, arg))
            || self
                .instantiated_generic_method_args
                .iter()
                .any(|&arg| self.type_contains_instantiated_generic_arg(target_type, arg))
    }

    fn type_contains_instantiated_generic_arg(&self, type_id: TypeId, arg: TypeId) -> bool {
        if self.type_matches_instantiated_generic_arg(type_id, arg) {
            return true;
        }
        if crate::visitor::application_id(self.interner, arg).is_none() {
            return false;
        }
        let mut found = false;
        crate::visitor::walk_referenced_types(self.interner, type_id, |candidate| {
            if self.type_matches_instantiated_generic_arg(candidate, arg) {
                found = true;
            }
        });
        found
    }

    fn type_matches_instantiated_generic_arg(&self, type_id: TypeId, arg: TypeId) -> bool {
        if type_id == arg {
            return true;
        }

        if let Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) =
            self.interner.lookup(type_id)
        {
            return self.type_matches_instantiated_generic_arg(inner, arg);
        }

        if let Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) =
            self.interner.lookup(arg)
        {
            return self.type_matches_instantiated_generic_arg(type_id, inner);
        }

        self.interner.get_display_alias(type_id) == Some(arg)
            || self.interner.get_display_alias(arg) == Some(type_id)
    }

    /// Check if `this` parameters are compatible.
    ///
    /// TypeScript only checks `this` parameter compatibility when the target
    /// declares an explicit `this` parameter. If the target has no `this` parameter,
    /// any source `this` type is acceptable.
    /// Check if this-parameters are compatible between source and target.
    ///
    /// When `is_method` is true, this-parameters are bivariant regardless of
    /// `strict_function_types`, matching tsc's behavior where method this-parameters
    /// follow method parameter bivariance.
    pub(crate) fn are_this_parameters_compatible(
        &mut self,
        source_type: Option<TypeId>,
        target_type: Option<TypeId>,
        is_method: bool,
    ) -> bool {
        // If target has no explicit `this` parameter, always compatible.
        // TypeScript only checks `this` when the target declares one.
        if target_type.is_none() {
            return true;
        }
        let source_type = source_type.unwrap_or(TypeId::UNKNOWN);
        let target_type = target_type.unwrap_or(TypeId::UNKNOWN);

        // this parameters follow the same variance rules as regular parameters.
        // For methods, this-parameters are bivariant (matching method parameter bivariance).
        if is_method {
            // Bivariant for methods
            self.check_subtype(source_type, target_type).is_true()
                || self.check_subtype(target_type, source_type).is_true()
        } else if self.strict_function_types {
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
    /// TypeScript treats a source optional parameter as accepting explicit
    /// `undefined` when it is compared against a required target parameter.
    /// This keeps `(x?: T) => void` assignable to `(x: T | undefined) => void`
    /// under strict function contravariance.
    ///
    /// A target optional parameter still compares by its declared type rather
    /// than eagerly widening to `T | undefined`. That keeps `(x: string) =>
    /// void` assignable to `(x?: string) => void`, matching tsc's behavior for
    /// regular function signature relation checks.
    ///
    /// When both parameters are optional, strip `undefined` from their types
    /// so `(x?: T)` and `(x?: T | undefined)` compare as equivalent. This
    /// matches tsc's behavior where both forms are interchangeable in
    /// signature comparison.
    ///
    /// When only the source parameter is optional, add `undefined` to the source
    /// type. Other one-sided optional comparisons keep their declared types,
    /// preserving the stricter comparison needed to catch legitimate
    /// undefined-related mismatches.
    pub(crate) fn effective_param_type_pair(
        &self,
        s_param: &ParamInfo,
        t_param: &ParamInfo,
    ) -> (TypeId, TypeId) {
        match (s_param.optional, t_param.optional) {
            (true, true) => (
                self.strip_undefined_from_param_type(s_param.type_id),
                self.strip_undefined_from_param_type(t_param.type_id),
            ),
            (true, false) => (
                self.add_undefined_to_param_type(s_param.type_id),
                t_param.type_id,
            ),
            _ => (s_param.type_id, t_param.type_id),
        }
    }

    /// Add `undefined` to an optional source parameter type for signature
    /// compatibility. `union2` canonicalizes duplicate `undefined` members.
    fn add_undefined_to_param_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner.union2(type_id, TypeId::UNDEFINED)
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

        // Two independent reasons to re-check the raw deferred forms instead
        // of trusting an `unknown` evaluation of an `Application`/`Lazy`
        // return:
        //
        // 1. Placeholder collapse: the reference is unresolvable (no defining
        //    `DefId`, no registered body, an `unknown` cross-file placeholder
        //    body, or a self-referential `Lazy` wrapper), so its `unknown` is
        //    a missing-body sentinel — see
        //    [`Self::return_type_needs_raw_fallback`].
        //
        // 2. Recursive-application cycle guard: the evaluation collapsed to
        //    `unknown` because a recursion guard bailed (e.g. self-referential
        //    iterator applications). The collapsed `unknown` is a cycle
        //    artifact, not the converged answer — relating against it would
        //    accept anything.
        //
        // Neither guard may fire for a *stable, resolvable* evaluation. A
        // deferred alias application that genuinely evaluates to `unknown`
        // (e.g. a conditional alias whose selected branch is `unknown`) is
        // the converged answer: tsc relates the source against that `unknown`
        // (everything is assignable). Vetoing it through a raw-form
        // comparison manufactures `X is not assignable to unknown` false
        // positives (#13212).
        let placeholder_fallback = self.return_type_needs_raw_fallback(source_return)
            || self.return_type_needs_raw_fallback(target_return);
        let mut unstable_unknown_collapse = |ret: TypeId| {
            matches!(
                self.interner.lookup(ret),
                Some(TypeData::Application(_) | TypeData::Lazy(_))
            ) && self.evaluate_type_with_stability(ret).is_unstable_unknown()
        };
        let needs_raw_fallback = placeholder_fallback
            || unstable_unknown_collapse(source_return)
            || unstable_unknown_collapse(target_return);

        if needs_raw_fallback {
            let prev = self.bypass_evaluation;
            self.bypass_evaluation = true;
            let raw_result = self.check_subtype(source_return, target_return);
            self.bypass_evaluation = prev;
            if raw_result.is_false() {
                return raw_result;
            }
        }

        if let Some(&original_strict_function_types) = self.method_bivariance_strict_stack.last() {
            let saved_strict_function_types = self.strict_function_types;
            self.strict_function_types = original_strict_function_types;
            let result = self.check_subtype(source_return, target_return);
            self.strict_function_types = saved_strict_function_types;
            result
        } else {
            self.check_subtype(source_return, target_return)
        }
    }

    /// Whether a return type must be compared in its raw alias form by
    /// [`Self::check_return_compat`]: an `Application`/`Lazy` reference whose
    /// evaluation produced `unknown` as a placeholder for a missing body,
    /// rather than as a genuine evaluation result.
    ///
    /// A reference the resolver maps to a real body (e.g. `C<number>` where
    /// `type C<T> = T extends 1 ? unknown : unknown`) makes the evaluator's
    /// `unknown` answer authoritative: the relation must compare the
    /// evaluated form (`unknown` relates to `unknown`), not the raw alias
    /// shape. Only an unresolvable reference — no defining `DefId`, no
    /// registered body, an `unknown` placeholder body (cross-file body not
    /// yet registered), or a self-referential `Lazy` wrapper — justifies the
    /// raw-form fallback.
    ///
    /// Detecting a placeholder also records an unresolved-lazy relation event:
    /// any relation result derived from the raw comparison is
    /// schedule-dependent and must not be memoized as definitive in the shared
    /// relation cache.
    fn return_type_needs_raw_fallback(&mut self, return_type: TypeId) -> bool {
        // Single interner lookup yields both the alias-shape gate and the
        // defining `DefId` (this runs for every function-pair return check).
        let def_id = match self.interner.lookup(return_type) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::Application(app_id)) => {
                let base = self.interner.type_application(app_id).base;
                crate::visitor::lazy_def_id(self.interner, base)
            }
            _ => return false,
        };
        if self.evaluate_type(return_type) != TypeId::UNKNOWN {
            return false;
        }
        let is_placeholder = match def_id {
            Some(def_id) => match self.resolver.resolve_lazy(def_id, self.interner) {
                // A resolved body of `unknown` is ambiguous: it is EITHER a
                // cross-file registration-window placeholder (the declaring file
                // has not published its real body yet, so its `unknown` is a
                // missing-body sentinel) OR a genuine, finalized `unknown` body
                // (`type C<T> = unknown`, or a utility alias that reduces to
                // `unknown`). Only the placeholder justifies the raw-form
                // fallback; for a genuine body the evaluator's `unknown` is
                // authoritative and the source must relate to it (everything is
                // assignable to `unknown`). Mirror the genuine-vs-placeholder
                // distinction made by `evaluate_application` via the shared
                // `is_genuine_unknown_alias_body` predicate (issue #14595 /
                // #13212). Without this guard a function-typed member returning
                // such an alias (`run: () => C<T>`) reaches this check with a
                // genuine `unknown` body, is misclassified as a placeholder, and
                // the raw deferred `Application` is compared against the source —
                // a false `unknown` ≰ `C<...>` (TS2322) in function-return
                // position.
                Some(body) if body == TypeId::UNKNOWN => !self
                    .resolver
                    .is_genuine_unknown_alias_body(def_id, self.interner),
                // A self-referential `Lazy` wrapper is a structural body not yet
                // materialized on this query — also a placeholder.
                Some(body) => matches!(
                    self.interner.lookup(body),
                    Some(TypeData::Lazy(body_def)) if body_def == def_id
                ),
                None => true,
            },
            None => true,
        };
        if is_placeholder {
            self.note_unresolved_lazy_relation_event();
        }
        is_placeholder
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
                type_id: instantiate_type(self.interner, p.type_id, substitution),
                ..*p
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

    /// Return the binder carried by a bare variadic rest type.
    ///
    /// `NoInfer<T>` is still the same opaque variadic slot as `T`; the wrapper
    /// only affects inference. Array, tuple, and other structural wrappers are
    /// deliberately not peeled. Call-local and higher-order inference
    /// placeholders stay provisional: they are not universally quantified
    /// source binders and must keep participating in ordinary inference.
    fn bare_rest_type_param(&mut self, type_id: TypeId) -> Option<TypeParamInfo> {
        if let Some(query_db) = self.query_db {
            return match crate::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                query_db,
                self.resolver,
                type_id,
            ) {
                crate::type_queries::RestBinderQuery::Complete(value) => value,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    None
                }
            };
        }
        self.bare_rest_type_param_inner(type_id)
    }

    fn bare_rest_type_param_inner(&mut self, type_id: TypeId) -> Option<TypeParamInfo> {
        let mut current = type_id;
        let mut seen = FxHashSet::default();
        for _ in 0..crate::type_queries::data::MAX_REST_BINDER_QUERY_STEPS {
            if current.is_intrinsic() || !seen.insert(current) {
                return None;
            }
            match self.interner.lookup(current) {
                Some(TypeData::TypeParameter(info)) if !info.is_infer_placeholder() => {
                    return Some(info);
                }
                Some(TypeData::NoInfer(inner)) => current = inner,
                Some(TypeData::Substitution { base_type, .. }) => current = base_type,
                Some(TypeData::Application(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) => {
                    let evaluated = self.evaluate_type(current);
                    if evaluated == current || evaluated == TypeId::ERROR {
                        return None;
                    }
                    current = evaluated;
                }
                _ => return None,
            }
        }
        self.note_incomplete_evaluation_relation_event();
        None
    }

    pub(crate) fn is_bare_rest_type_param(&mut self, type_id: TypeId) -> bool {
        if let Some(query_db) = self.query_db {
            return match crate::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                query_db,
                self.resolver,
                type_id,
            ) {
                crate::type_queries::RestBinderQuery::Complete(value) => value.is_some(),
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    true
                }
            };
        }
        self.bare_rest_type_param_inner(type_id).is_some()
    }

    fn is_unresolved_bare_rest(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => true,
            Some(TypeData::NoInfer(inner)) => self.is_unresolved_bare_rest(inner),
            Some(TypeData::Substitution { base_type, .. }) => {
                self.is_unresolved_bare_rest(base_type)
            }
            _ => false,
        }
    }

    fn single_variadic_tuple_rest_binder(&mut self, type_id: TypeId) -> Option<TypeParamInfo> {
        if let Some(query_db) = self.query_db {
            return match crate::type_queries::single_variadic_tuple_rest_type_parameter_with_resolver_query(
                query_db,
                self.resolver,
                type_id,
            ) {
                crate::type_queries::RestBinderQuery::Complete(value) => value,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    None
                }
            };
        }
        let TypeData::Tuple(elements_id) = self.interner.lookup(type_id)? else {
            return None;
        };
        let elements = self.interner.tuple_list(elements_id);
        let [element] = &*elements else {
            return None;
        };
        (element.rest && !element.optional)
            .then(|| self.bare_rest_type_param_inner(element.type_id))
            .flatten()
    }

    fn is_concrete_any_array_rest(&mut self, type_id: TypeId) -> bool {
        let mut current = type_id;
        let mut seen = FxHashSet::default();
        for _ in 0..crate::type_queries::data::MAX_REST_BINDER_QUERY_STEPS {
            if current.is_intrinsic() || !seen.insert(current) {
                return false;
            }
            match self.interner.lookup(current) {
                Some(TypeData::Array(element)) => return element == TypeId::ANY,
                Some(TypeData::NoInfer(inner)) => current = inner,
                Some(TypeData::Application(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) => {
                    let evaluated = self.evaluate_type(current);
                    if evaluated == current || evaluated == TypeId::ERROR {
                        return false;
                    }
                    current = evaluated;
                }
                _ => return false,
            }
        }
        self.note_incomplete_evaluation_relation_event();
        false
    }

    pub(crate) fn rest_type_has_union_surface(&mut self, type_id: TypeId) -> bool {
        if let Some(query_db) = self.query_db {
            return match crate::type_queries::rest_type_has_union_surface_with_resolver_query(
                query_db,
                self.resolver,
                type_id,
            ) {
                crate::type_queries::RestBinderQuery::Complete(value) => value,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    false
                }
            };
        }

        let mut current = type_id;
        let mut seen = FxHashSet::default();
        for _ in 0..crate::type_queries::data::MAX_REST_BINDER_QUERY_STEPS {
            if current.is_intrinsic() || !seen.insert(current) {
                return false;
            }
            match self.interner.lookup(current) {
                Some(TypeData::Union(_)) => return true,
                Some(TypeData::NoInfer(inner)) => current = inner,
                Some(TypeData::Substitution { constraint, .. }) => current = constraint,
                Some(TypeData::Application(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) => {
                    let evaluated = self.evaluate_type(current);
                    if evaluated == current || evaluated == TypeId::ERROR {
                        return false;
                    }
                    current = evaluated;
                }
                _ => return false,
            }
        }
        self.note_incomplete_evaluation_relation_event();
        false
    }

    /// Decide the raw-rest relation for an opaque source variadic.
    ///
    /// A bare source `...T` is universally quantified. It cannot be projected
    /// through `T`'s constraint and compared element-wise with an unrelated
    /// rest shape. Compare the raw rest types through the ordinary parameter
    /// relation so strictness, method bivariance, `NoInfer`, and transparent
    /// tuple spreads retain their existing semantics. Concrete `any[]` remains
    /// TypeScript's universal callable-rest exception.
    ///
    /// `None` means the source rest is structural rather than a bare type
    /// parameter, so ordinary element-wise comparison should continue.
    pub(crate) fn bare_source_rest_compatibility(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        is_method: bool,
        allow_provisional_union: bool,
    ) -> Option<bool> {
        let source_binder = self.bare_rest_type_param(source_type)?;
        let target_binder = self.bare_rest_type_param(target_type);
        let target_variadic_tuple_binder = self.single_variadic_tuple_rest_binder(target_type);
        if target_binder
            .into_iter()
            .chain(target_variadic_tuple_binder)
            .any(|target_binder| source_binder.is_same_binder(target_binder))
        {
            return Some(true);
        }
        if allow_provisional_union {
            debug_assert!(
                self.rest_type_has_union_surface(target_type),
                "the provisional bare-rest escape is scoped to a union target"
            );
            return None;
        }
        if self.is_concrete_any_array_rest(target_type) {
            return Some(true);
        }
        let strict_parameter_relation = self.force_strict_callback_param_variance
            || (self.strict_function_types && (!is_method || self.disable_method_bivariance));
        if strict_parameter_relation && self.rest_type_has_union_surface(target_type) {
            return Some(false);
        }
        Some(self.are_parameters_compatible_impl(source_type, target_type, is_method))
    }

    fn normalize_rest_params(&mut self, params: &mut [ParamInfo]) {
        for param in params {
            if !param.rest {
                continue;
            }
            if self.is_unresolved_bare_rest(param.type_id) {
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

    pub(crate) fn normalize_rest_param_types(&mut self, shape: &mut FunctionShape) {
        self.normalize_rest_params(&mut shape.params);
    }

    /// Expand tuple-list rest parameters after their application surfaces have
    /// been normalized. This is shared by direct signature comparison and the
    /// contextual-retry guard so both observe the same logical slots.
    pub(crate) fn unpack_normalized_params(&mut self, params: &[ParamInfo]) -> Vec<ParamInfo> {
        use crate::type_queries::unpack_tuple_rest_parameter;

        params
            .iter()
            .flat_map(|param| {
                if param.rest
                    && matches!(
                        self.interner.lookup(param.type_id),
                        Some(TypeData::Application(_))
                    )
                {
                    let evaluated = self.evaluate_type(param.type_id);
                    if evaluated != param.type_id {
                        let mut evaluated_param = *param;
                        evaluated_param.type_id = evaluated;
                        return unpack_tuple_rest_parameter(self.interner, &evaluated_param);
                    }
                }
                unpack_tuple_rest_parameter(self.interner, param)
            })
            .collect()
    }

    pub(crate) fn normalized_unpacked_params(&mut self, params: &[ParamInfo]) -> Vec<ParamInfo> {
        let mut normalized = params.to_vec();
        self.normalize_rest_params(&mut normalized);
        self.unpack_normalized_params(&normalized)
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
        let mut rename_substitution = TypeSubstitution::for_signature_domain(&source.type_params);
        let mut renamed_type_params = Vec::with_capacity(source.type_params.len());
        let mut rename_buf = String::with_capacity(32);
        for (index, tp) in source.type_params.iter().enumerate() {
            rename_buf.clear();
            write!(rename_buf, "__infer_src_ctx_{index}").expect("write to String is infallible");
            let fresh_name = self.interner.intern_string(&rename_buf);
            // Legacy index-named source placeholder: classified as a higher-order
            // source placeholder but carries no origin name (matches the historical
            // `decode_src_placeholder_origin` returning `None` for `__infer_src_ctx_*`).
            let ctx_origin = crate::types::TypeParamOrigin::InferSource {
                id: index as u64,
                origin_name: None,
            };
            let fresh_type = self.interner.type_param(TypeParamInfo {
                name: fresh_name,
                constraint: None,
                default: None,
                is_const: tp.is_const,
                origin: ctx_origin,
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
                origin: ctx_origin,
            });
        }
        let renamed_source = FunctionShape {
            type_params: renamed_type_params,
            params: source
                .params
                .iter()
                .map(|p| ParamInfo {
                    type_id: instantiate_type(self.interner, p.type_id, &rename_substitution),
                    ..*p
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

        // A constraint that references the signature's own (renamed) type
        // parameters cannot be enforced as an upper bound during resolution:
        // the bound still contains an unresolved rename placeholder, so a
        // perfectly valid inference such as `T := S` for
        // `<T extends { value: T }>` would be rejected by the literal check
        // `S <: { value: __infer_src_ctx_0 }`. tsc instead validates the
        // inferred type against the constraint *instantiated with the
        // inference mapper* (`instantiateType(constraint, mapper)`); mirror
        // that by deferring self-referential constraints and checking them
        // once the full substitution is known (below, before returning).
        let renamed_param_names: FxHashSet<Atom> = renamed_source
            .type_params
            .iter()
            .map(|tp| tp.name)
            .collect();
        let mut deferred_self_referential_constraints: Vec<(Atom, TypeId)> = Vec::new();
        let mut infer_ctx = InferenceContext::new(self.interner);
        for tp in &renamed_source.type_params {
            let var = infer_ctx.fresh_type_param(tp.name, tp.is_const);
            if let Some(constraint) = tp.constraint {
                if crate::visitors::visitor_predicates::references_any_type_param_named(
                    self.interner,
                    constraint,
                    &renamed_param_names,
                ) {
                    deferred_self_referential_constraints.push((tp.name, constraint));
                } else {
                    infer_ctx.add_upper_bound(var, constraint);
                    infer_ctx.set_declared_constraint(var, constraint);
                }
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
                let was_variance_walk = infer_ctx.in_variance_walk;
                infer_ctx.in_contra_mode = true;
                infer_ctx.in_variance_walk = true;
                let _ = infer_ctx.infer_from_types(
                    s_effective,
                    t_effective,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
                infer_ctx.in_variance_walk = was_variance_walk;
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
                let was_variance_walk = infer_ctx.in_variance_walk;
                infer_ctx.in_contra_mode = true;
                infer_ctx.in_variance_walk = true;
                let _ = infer_ctx.infer_from_types(
                    s_param.type_id,
                    rest_elem_type,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
                infer_ctx.in_variance_walk = was_variance_walk;
            }

            if source_has_rest && let Some(s_rest_param) = source_params_unpacked.last() {
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                let was_contra = infer_ctx.in_contra_mode;
                let was_variance_walk = infer_ctx.in_variance_walk;
                infer_ctx.in_contra_mode = true;
                infer_ctx.in_variance_walk = true;
                let _ = infer_ctx.infer_from_types(
                    s_rest_elem,
                    rest_elem_type,
                    InferencePriority::NakedTypeVariable,
                );
                infer_ctx.in_contra_mode = was_contra;
                infer_ctx.in_variance_walk = was_variance_walk;
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
                    let was_variance_walk = infer_ctx.in_variance_walk;
                    infer_ctx.in_contra_mode = true;
                    infer_ctx.in_variance_walk = true;
                    let _ = infer_ctx.infer_from_types(
                        rest_elem_type,
                        t_param.type_id,
                        InferencePriority::NakedTypeVariable,
                    );
                    infer_ctx.in_contra_mode = was_contra;
                    infer_ctx.in_variance_walk = was_variance_walk;
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
        let mut substitution = TypeSubstitution::for_signature_domain(&source.type_params);
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
                        self.type_param_appears_in_mapped_context(param.type_id, *original_tp)
                    }) || source.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, *original_tp)
                    }) || self
                        .type_param_appears_in_mapped_context(source.return_type, *original_tp)));
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

        // Deferred self-referential constraint validation (see the
        // registration loop above): check each inferred type against its
        // declared constraint instantiated with the full inference solution,
        // mirroring tsc's `instantiateType(constraint, mapper)` check. For
        // `<T extends { value: T }>` inferred as `T := S` this validates
        // `S <: { value: S }` instead of the unsatisfiable
        // `S <: { value: __infer_src_ctx_0 }`.
        if !deferred_self_referential_constraints.is_empty() {
            let mut renamed_solution =
                TypeSubstitution::for_signature_domain(&renamed_source.type_params);
            for (original_tp, renamed_tp) in source
                .type_params
                .iter()
                .zip(renamed_source.type_params.iter())
            {
                if let Some(resolved) = substitution.get(original_tp.name) {
                    renamed_solution.insert(renamed_tp.name, resolved);
                }
            }
            for (renamed_name, constraint) in &deferred_self_referential_constraints {
                let Some(inferred_ty) = renamed_solution.get(*renamed_name) else {
                    continue;
                };
                if inferred_ty.is_any_unknown_or_error() {
                    continue;
                }
                let instantiated_constraint =
                    instantiate_type(self.interner, *constraint, &renamed_solution);
                if !self
                    .check_subtype(inferred_ty, instantiated_constraint)
                    .is_true()
                {
                    return Err(crate::inference::infer::InferenceError::Conflict(
                        inferred_ty,
                        instantiated_constraint,
                    ));
                }
            }
        }
        Ok(substitution)
    }
}

mod checking;
