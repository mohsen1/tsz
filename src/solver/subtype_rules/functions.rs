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

use std::collections::HashSet;

use crate::solver::types::*;

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
                // First, check if predicates target the same parameter
                if source_pred.target != target_pred.target {
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
        let mut visited: HashSet<TypeId> = HashSet::new();
        self.type_contains_this_type_inner(type_id, &mut visited)
    }

    /// Recursive helper for checking if a type contains `this`.
    pub(crate) fn type_contains_this_type_inner(
        &self,
        type_id: TypeId,
        visited: &mut HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::ThisType => true,
            TypeKey::Array(elem) => self.type_contains_this_type_inner(elem, visited),
            TypeKey::Tuple(list_id) => {
                let elements = self.interner.tuple_list(list_id);
                elements
                    .iter()
                    .any(|elem| self.type_contains_this_type_inner(elem.type_id, visited))
            }
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&member| self.type_contains_this_type_inner(member, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                })
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                }) {
                    return true;
                }
                if let Some(index) = &shape.string_index
                    && (self.type_contains_this_type_inner(index.key_type, visited)
                        || self.type_contains_this_type_inner(index.value_type, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.number_index
                    && (self.type_contains_this_type_inner(index.key_type, visited)
                        || self.type_contains_this_type_inner(index.value_type, visited))
                {
                    return true;
                }
                false
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_this_type_inner(this_type, visited)
                    })
                    || self.type_contains_this_type_inner(shape.return_type, visited)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                if shape.call_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_this_type_inner(this_type, visited)
                        })
                        || self.type_contains_this_type_inner(sig.return_type, visited)
                }) {
                    return true;
                }
                if shape.construct_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_this_type_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_this_type_inner(this_type, visited)
                        })
                        || self.type_contains_this_type_inner(sig.return_type, visited)
                }) {
                    return true;
                }
                shape.properties.iter().any(|prop| {
                    self.type_contains_this_type_inner(prop.type_id, visited)
                        || self.type_contains_this_type_inner(prop.write_type, visited)
                })
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                info.constraint.is_some_and(|constraint| {
                    self.type_contains_this_type_inner(constraint, visited)
                }) || info
                    .default
                    .is_some_and(|default| self.type_contains_this_type_inner(default, visited))
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_this_type_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_this_type_inner(arg, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_this_type_inner(cond.check_type, visited)
                    || self.type_contains_this_type_inner(cond.extends_type, visited)
                    || self.type_contains_this_type_inner(cond.true_type, visited)
                    || self.type_contains_this_type_inner(cond.false_type, visited)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_this_type_inner(constraint, visited)
                }) || mapped
                    .type_param
                    .default
                    .is_some_and(|default| self.type_contains_this_type_inner(default, visited))
                    || self.type_contains_this_type_inner(mapped.constraint, visited)
                    || mapped.name_type.is_some_and(|name_type| {
                        self.type_contains_this_type_inner(name_type, visited)
                    })
                    || self.type_contains_this_type_inner(mapped.template, visited)
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_this_type_inner(obj, visited)
                    || self.type_contains_this_type_inner(idx, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.type_contains_this_type_inner(inner, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_this_type_inner(*inner, visited)
                    }
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.type_contains_this_type_inner(type_arg, visited)
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ModuleNamespace(_)
            | TypeKey::Error => false,
        }
    }

    /// Check if `this` parameters are compatible.
    pub(crate) fn are_this_parameters_compatible(
        &mut self,
        source_type: Option<TypeId>,
        target_type: Option<TypeId>,
    ) -> bool {
        if source_type.is_none() && target_type.is_none() {
            return true;
        }
        // Use Unknown instead of Any for stricter type checking
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
    pub(crate) fn check_function_subtype(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        // Constructor vs non-constructor
        if source.is_constructor != target.is_constructor {
            return SubtypeResult::False;
        }

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

        // Type predicates check
        if !self.are_type_predicates_compatible(source, target) {
            return SubtypeResult::False;
        }

        // Method bivariance
        let is_method = source.is_method || target.is_method;

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

        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

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

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source.params[i];
            let t_param = &target.params[i];

            // Check optional compatibility
            if s_param.optional && !t_param.optional {
                if !self
                    .check_subtype(TypeId::UNDEFINED, t_param.type_id)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }

            // Check parameter compatibility
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
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
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

        // Check properties (if any), excluding private fields
        let source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
            .cloned()
            .collect();
        let target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !self.interner.resolve_atom(p.name).starts_with('#'))
            .cloned()
            .collect();
        if !self
            .check_object_subtype(&source_props, None, &target_props)
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

        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

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
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
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

        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

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
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
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

        let source_required = self.required_param_count(&source.params);
        let target_required = self.required_param_count(&target.params);
        let extra_required_ok = target_has_rest
            && source_required > target_required
            && self.extra_required_accepts_undefined(
                &source.params,
                target_required,
                source_required,
            );
        if !self.allow_bivariant_param_count
            && !rest_is_top
            && source_required > target_required
            && (!target_has_rest || !extra_required_ok)
        {
            return SubtypeResult::False;
        }

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
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
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

    /// Evaluate a meta-type (conditional, index access, mapped, etc.) to its concrete form.
    /// Uses TypeEvaluator to reduce types like `T extends U ? X : Y` to either X or Y.
    ///
    /// When a QueryDatabase is available (via `with_query_db`), routes through Salsa
    /// for memoization. Otherwise falls back to creating a fresh TypeEvaluator.
    pub(crate) fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        if let Some(db) = self.query_db {
            return db.evaluate_type(type_id);
        }
        use crate::solver::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        evaluator.evaluate(type_id)
    }
}
