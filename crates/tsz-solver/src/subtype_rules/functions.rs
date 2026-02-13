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

use crate::infer::InferenceContext;
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::*;
use crate::visitor::contains_this_type;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if parameter types are compatible based on variance settings.
    ///
    /// In strict mode (contravariant): target_type <: source_type
    /// In legacy mode (bivariant): target_type <: source_type OR source_type <: target_type
    /// See https://github.com/microsoft/TypeScript/issues/18654.
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
            (None, None) => true,

            // Source has predicate, target doesn't - allow assignment.
            // Type predicates are implemented as runtime boolean returns, so a function with
            // a predicate is still callable where a plain boolean-returning function is
            // expected (as in ReturnType<T>).
            (Some(_), None) => true,

            // Source has no predicate, target has one - still compatible.
            // This mirrors TypeScript's behavior: a less specific function (no predicate)
            // can be used where a more specific function (with a predicate) is expected.
            (None, Some(_)) => true,

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
                    (false, true) => false,
                    // Source is assertion, target is type guard - NOT compatible
                    (true, false) => false,
                    // Both same type - check type compatibility
                    (false, false) | (true, true) => {
                        match (source_pred.type_id, target_pred.type_id) {
                            (Some(source_type), Some(target_type)) => {
                                self.check_subtype(source_type, target_type).is_true()
                            }
                            (None, Some(_)) => false,
                            (Some(_), None) => true,
                            (None, None) => true,
                        }
                    }
                }
            }
        }
    }

    /// Check parameter compatibility with method bivariance support.
    /// Methods are bivariant even when strict_function_types is enabled.
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

        let contains_this =
            self.type_contains_this_type(source_type) || self.type_contains_this_type(target_type);

        // Methods are bivariant regardless of strict_function_types setting
        // UNLESS disable_method_bivariance is set
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
        params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count()
    }

    /// Check if extra required parameters accept undefined.
    pub(crate) fn extra_required_accepts_undefined(
        &mut self,
        params: &[ParamInfo],
        from_index: usize,
        required_count: usize,
    ) -> bool {
        params
            .iter()
            .take(required_count)
            .skip(from_index)
            .all(|param| {
                self.check_subtype(TypeId::UNDEFINED, param.type_id)
                    .is_true()
            })
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

    fn infer_source_type_param_substitution(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> TypeSubstitution {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut infer_ctx = InferenceContext::new(self.interner);
        for tp in &source.type_params {
            let var = infer_ctx.fresh_type_param(tp.name, tp.is_const);
            if let Some(constraint) = tp.constraint {
                infer_ctx.add_upper_bound(var, constraint);
            }
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

            let _ = infer_ctx.infer_from_types(
                t_effective,
                s_effective,
                InferencePriority::NakedTypeVariable,
            );
        }

        if target_has_rest && let Some(rest_elem_type) = rest_elem_type {
            for s_param in source_params_unpacked
                .iter()
                .take(source_fixed_count)
                .skip(target_fixed_count)
            {
                let _ = infer_ctx.infer_from_types(
                    rest_elem_type,
                    s_param.type_id,
                    InferencePriority::NakedTypeVariable,
                );
            }

            if source_has_rest && let Some(s_rest_param) = source_params_unpacked.last() {
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                let _ = infer_ctx.infer_from_types(
                    rest_elem_type,
                    s_rest_elem,
                    InferencePriority::NakedTypeVariable,
                );
            }
        }

        if source_has_rest && let Some(rest_param) = source_params_unpacked.last() {
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            for t_param in target_params_unpacked
                .iter()
                .take(target_fixed_count)
                .skip(source_fixed_count)
            {
                let _ = infer_ctx.infer_from_types(
                    t_param.type_id,
                    rest_elem_type,
                    InferencePriority::NakedTypeVariable,
                );
            }
        }

        let _ = infer_ctx.infer_from_types(
            target.return_type,
            source.return_type,
            InferencePriority::ReturnType,
        );
        if let (Some(source_this), Some(target_this)) = (source.this_type, target.this_type) {
            let _ = infer_ctx.infer_from_types(
                target_this,
                source_this,
                InferencePriority::NakedTypeVariable,
            );
        }
        if let (Some(source_pred), Some(target_pred)) =
            (&source.type_predicate, &target.type_predicate)
            && let (Some(source_ty), Some(target_ty)) = (source_pred.type_id, target_pred.type_id)
        {
            let _ = infer_ctx.infer_from_types(target_ty, source_ty, InferencePriority::ReturnType);
        }

        let inferred = infer_ctx.resolve_all_with_constraints().unwrap_or_default();
        let mut substitution = TypeSubstitution::new();
        for tp in &source.type_params {
            let inferred_ty = inferred
                .iter()
                .find_map(|(name, ty)| if *name == tp.name { Some(*ty) } else { None });
            let fallback = if self.strict_function_types {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };
            substitution.insert(tp.name, inferred_ty.unwrap_or(fallback));
        }
        substitution
    }

    /// Check if a function type is a subtype of another function type.
    ///
    /// Validates function compatibility by checking:
    /// - Constructor/non-constructor matching
    /// - Return type compatibility (covariant)
    /// - `this` parameter compatibility
    /// - Type predicate compatibility
    /// - Parameter compatibility (contravariant or bivariant for methods)
    /// - Rest parameter handling
    /// - Optional parameter compatibility
    ///
    /// Generic instantiation: When the target is generic but the source is not,
    /// we instantiate the target's type parameters to `any` before checking compatibility.
    /// This allows non-generic implementations to be compatible with generic overloads.
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

        // Generic source vs generic target (same arity): normalize target type parameter
        // identities to source identities so alpha-equivalent signatures compare structurally.
        if !source_instantiated.type_params.is_empty()
            && source_instantiated.type_params.len() == target_instantiated.type_params.len()
            && !target_instantiated.type_params.is_empty()
        {
            let mut substitution = TypeSubstitution::new();
            for (source_tp, target_tp) in source_instantiated
                .type_params
                .iter()
                .zip(target_instantiated.type_params.iter())
            {
                let source_type_param_type = self
                    .interner
                    .intern(TypeKey::TypeParameter(source_tp.clone()));
                substitution.insert(target_tp.name, source_type_param_type);
            }
            target_instantiated =
                self.instantiate_function_shape(&target_instantiated, &substitution);
        }

        // Contextual signature instantiation for generic source -> non-generic target.
        // This is key for non-strict assignability cases where a generic function expression
        // is contextually typed by a concrete callback/function type.
        if !source_instantiated.type_params.is_empty() && target_instantiated.type_params.is_empty()
        {
            let substitution = self
                .infer_source_type_param_substitution(&source_instantiated, &target_instantiated);
            source_instantiated =
                self.instantiate_function_shape(&source_instantiated, &substitution);
        }

        // Generic target vs non-generic source: instantiate target type params to `any`.
        if !target_instantiated.type_params.is_empty() && source_instantiated.type_params.is_empty()
        {
            let mut substitution = TypeSubstitution::new();
            for param in &target_instantiated.type_params {
                substitution.insert(param.name, TypeId::ANY);
            }
            target_instantiated =
                self.instantiate_function_shape(&target_instantiated, &substitution);
        }

        // Return type is covariant
        if !self
            .check_return_compat(
                source_instantiated.return_type,
                target_instantiated.return_type,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(
            source_instantiated.this_type,
            target_instantiated.this_type,
        ) {
            return SubtypeResult::False;
        }

        // Type predicates check
        if !self.are_type_predicates_compatible(&source_instantiated, &target_instantiated) {
            return SubtypeResult::False;
        }

        // Method bivariance
        let is_method = source_instantiated.is_method || target_instantiated.is_method;

        // Unpack tuple rest parameters before comparison.
        // In TypeScript, `(...args: [A, B]) => R` is equivalent to `(a: A, b: B) => R`.
        // We unpack tuple rest parameters into individual fixed parameters for proper matching.
        use crate::type_queries::unpack_tuple_rest_parameter;
        let source_params_unpacked: Vec<ParamInfo> = source_instantiated
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();
        let target_params_unpacked: Vec<ParamInfo> = target_instantiated
            .params
            .iter()
            .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
            .collect();

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
        let source_required = self.required_param_count(&source_params_unpacked);
        if !self.allow_bivariant_param_count
            && !target_has_rest
            && source_required > target_fixed_count
        {
            return SubtypeResult::False;
        }

        // Check parameter types
        // In strict function mode, temporarily use TopLevelOnly for any propagation
        // to prevent any from silencing structural mismatches in function parameters
        use crate::subtype::AnyPropagationMode;

        let old_mode = self.any_propagation;
        if self.strict_function_types {
            self.any_propagation = AnyPropagationMode::TopLevelOnly;
        }

        let param_check_result = (|| -> SubtypeResult {
            // Compare fixed parameters (using unpacked params)
            let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
            for i in 0..fixed_compare_count {
                let s_param = &source_params_unpacked[i];
                let t_param = &target_params_unpacked[i];

                // Optional parameters have effective type `T | undefined`.
                // TypeScript widens optional params to include undefined for
                // assignability checks.
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

                // Check parameter compatibility
                if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method) {
                    // Trace: Parameter type mismatch
                    if let Some(tracer) = &mut self.tracer {
                        if !tracer.on_mismatch_dyn(
                            crate::diagnostics::SubtypeFailureReason::ParameterTypeMismatch {
                                param_index: i,
                                source_param: s_param.type_id,
                                target_param: t_param.type_id,
                            },
                        ) {
                            return SubtypeResult::False;
                        }
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
                    return SubtypeResult::True;
                }

                for i in target_fixed_count..source_fixed_count {
                    let s_param = &source_params_unpacked[i];
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
                let rest_is_top = self.allow_bivariant_rest
                    && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

                if !rest_is_top {
                    for i in source_fixed_count..target_fixed_count {
                        let t_param = &target_params_unpacked[i];
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

        // Restore the original any_propagation mode
        self.any_propagation = old_mode;

        param_check_result
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
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
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

        for s_sig in &s_callable.call_signatures {
            if self
                .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                .is_true()
            {
                return SubtypeResult::True;
            }
        }
        SubtypeResult::False
    }

    /// Check callable subtyping with overloaded signatures.
    pub(crate) fn check_callable_subtype(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> SubtypeResult {
        // For each target call signature, at least one source signature must match
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

        // For each target construct signature, at least one source signature must match
        for t_sig in &target.construct_signatures {
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
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
        let mut source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
            .cloned()
            .collect();
        source_props.sort_by_key(|a| a.name);
        let mut target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
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
            .check_object_subtype(&source_shape, None, &target_shape)
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
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate.clone(),
            is_constructor: false,
            is_method: source.is_method,
        };
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate.clone(),
            is_constructor: false,
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
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target.this_type) {
            return SubtypeResult::False;
        }

        // Check rest parameter handling
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target
                .params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        // Count non-rest parameters
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };

        // Check parameter arity: source's required params must not exceed
        // the target's total non-rest params (including optional ones).
        // When target has a rest parameter, skip the arity check entirely —
        // the rest parameter can accept any number of arguments, and type
        // compatibility of extra params is checked later against the rest element type.
        let source_required = self.required_param_count(&source.params);
        if !self.allow_bivariant_param_count
            && !target_has_rest
            && source_required > target_fixed_count
        {
            return SubtypeResult::False;
        }

        // Compare fixed parameters
        // Methods use bivariant parameter checking (Rule #2: Function Bivariance)
        let is_method = source.is_method || target.is_method;
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            if !self.are_parameters_compatible_impl(s_param.type_id, t_param.type_id, is_method) {
                return SubtypeResult::False;
            }
        }

        // If target has rest, check source's extra params
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False;
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                // Check if source rest type is assignable to target rest type.
                // For tuple rest params like [...args: [T1, T2]], check the whole tuple
                // against the target array type, not just the first element.
                let target_rest_type = target.params.last().unwrap().type_id;
                if !self.are_parameters_compatible_impl(
                    s_rest_param.type_id,
                    target_rest_type,
                    is_method,
                ) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
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

    /// Check function shape subtype to call signature.
    pub(crate) fn check_call_signature_subtype_fn(
        &mut self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> SubtypeResult {
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target.this_type) {
            return SubtypeResult::False;
        }

        // Check rest parameter handling
        let target_has_rest = target.params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target
                .params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        // Count non-rest parameters
        let target_fixed_count = if target_has_rest {
            target.params.len().saturating_sub(1)
        } else {
            target.params.len()
        };
        let source_fixed_count = if source_has_rest {
            source.params.len().saturating_sub(1)
        } else {
            source.params.len()
        };

        // Check parameter arity: source's required params must not exceed
        // the target's total non-rest params (including optional ones).
        // When target has a rest parameter, skip the arity check entirely —
        // the rest parameter can accept any number of arguments, and type
        // compatibility of extra params is checked later against the rest element type.
        let source_required = self.required_param_count(&source.params);
        if !self.allow_bivariant_param_count
            && !target_has_rest
            && source_required > target_fixed_count
        {
            return SubtypeResult::False;
        }

        // Compare fixed parameters
        // Methods use bivariant parameter checking (Rule #2: Function Bivariance)
        let is_method = source.is_method || target.is_method;
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];
            if !self.are_parameters_compatible_impl(s_param.type_id, t_param.type_id, is_method) {
                return SubtypeResult::False;
            }
        }

        // If target has rest, check source's extra params
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False;
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for i in target_fixed_count..source_fixed_count {
                let s_param = &source.params[i];
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source.params.last() else {
                    return SubtypeResult::False;
                };
                // Check if source rest type is assignable to target rest type.
                // For tuple rest params like [...args: [T1, T2]], check the whole tuple
                // against the target array type, not just the first element.
                let target_rest_type = target.params.last().unwrap().type_id;
                if !self.are_parameters_compatible_impl(
                    s_rest_param.type_id,
                    target_rest_type,
                    is_method,
                ) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let rest_param = if let Some(rest_param) = source.params.last() {
                rest_param
            } else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest
                && (rest_elem_type == TypeId::ANY || rest_elem_type == TypeId::UNKNOWN);

            if !rest_is_top {
                for i in source_fixed_count..target_fixed_count {
                    let t_param = &target.params[i];
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
    /// concrete form. Uses TypeEvaluator with the resolver to correctly resolve
    /// Lazy(DefId) types at all nesting levels (e.g., KeyOf(Lazy(DefId))).
    ///
    /// Always uses TypeEvaluator with the resolver instead of query_db.evaluate_type()
    /// because the checker populates DefId→TypeId mappings in the TypeEnvironment that
    /// the query_db's resolver-less evaluator cannot access.
    ///
    /// Results are cached in eval_cache to avoid re-evaluating the same type across
    /// multiple subtype checks. This turns O(n²) evaluate calls into O(n).
    pub(crate) fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        // Check local evaluation cache first.
        // Key includes no_unchecked_indexed_access since it affects evaluation results.
        let cache_key = (type_id, self.no_unchecked_indexed_access);
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        use crate::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        let result = evaluator.evaluate(type_id);
        self.eval_cache.insert(cache_key, result);
        result
    }
}
