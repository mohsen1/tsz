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

use crate::instantiate::TypeSubstitution;
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

        // Handle generic target vs non-generic source
        // When checking if a non-generic implementation is compatible with a generic overload,
        // we need to instantiate the target's type parameters to `any` (or their constraints).
        // This implements universal quantification: the implementation must work for ALL possible T.
        let (target_return, target_this, target_params) =
            if !target.type_params.is_empty() && source.type_params.is_empty() {
                // Create a substitution mapping each type parameter to ANY
                let mut substitution = TypeSubstitution::new();
                for param in &target.type_params {
                    substitution.insert(param.name, TypeId::ANY);
                }

                // Instantiate target's return type, this_type, and parameters
                use crate::instantiate::instantiate_type;
                let instantiated_return =
                    instantiate_type(self.interner, target.return_type, &substitution);
                let instantiated_this = match target.this_type {
                    Some(this_id) => Some(instantiate_type(self.interner, this_id, &substitution)),
                    None => None,
                };

                // Instantiate parameters
                let instantiated_params: Vec<_> = target
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type(self.interner, p.type_id, &substitution),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect();

                (instantiated_return, instantiated_this, instantiated_params)
            } else {
                // Use the original target types
                (target.return_type, target.this_type, target.params.to_vec())
            };

        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target_return)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(source.this_type, target_this) {
            return SubtypeResult::False;
        }

        // Type predicates check
        if !self.are_type_predicates_compatible(source, target) {
            return SubtypeResult::False;
        }

        // Method bivariance
        let is_method = source.is_method || target.is_method;

        // Check rest parameter handling
        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source.params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        // Count non-rest parameters (needed for arity check below)
        let target_fixed_count = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
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

        // Check parameter types
        // In strict function mode, temporarily use TopLevelOnly for any propagation
        // to prevent any from silencing structural mismatches in function parameters
        use crate::subtype::AnyPropagationMode;

        let old_mode = self.any_propagation;
        if self.strict_function_types {
            self.any_propagation = AnyPropagationMode::TopLevelOnly;
        }

        let param_check_result = (|| -> SubtypeResult {
            // Compare fixed parameters
            let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
            for i in 0..fixed_compare_count {
                let s_param = &source.params[i];
                let t_param = &target_params[i];

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
                    let s_param = &source.params[i];
                    if !self.are_parameters_compatible_impl(
                        s_param.type_id,
                        rest_elem_type,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }

                if source_has_rest {
                    let Some(s_rest_param) = source.params.last() else {
                        return SubtypeResult::False;
                    };

                    // Handle tuple rest parameters - check tuple elements against target rest element
                    use crate::type_queries::get_tuple_list_id;
                    if let Some(s_tuple_id) = get_tuple_list_id(self.interner, s_rest_param.type_id)
                    {
                        // Source has tuple rest, target has array rest.
                        // In TypeScript, a tuple rest in source acts like a sequence of fixed parameters.
                        // Each element of the tuple must be compatible with the target's rest element type.
                        let s_tuple_elements = self.interner.tuple_list(s_tuple_id);
                        for s_elem in s_tuple_elements.iter() {
                            let s_elem_type = if s_elem.rest {
                                self.get_array_element_type(s_elem.type_id)
                            } else {
                                s_elem.type_id
                            };

                            if !self.are_parameters_compatible_impl(
                                s_elem_type,
                                rest_elem_type,
                                is_method,
                            ) {
                                return SubtypeResult::False;
                            }
                        }
                    } else {
                        // Both have array rest - check element types
                        let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                        if !self.are_parameters_compatible_impl(
                            s_rest_elem,
                            rest_elem_type,
                            is_method,
                        ) {
                            return SubtypeResult::False;
                        }
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
                        let t_param = &target_params[i];
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
        // Return type is covariant
        if !self
            .check_return_compat(source.return_type, target.return_type)
            .is_true()
        {
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
