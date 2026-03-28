//! Type-specific infer pattern matching helpers.
//!
//! Contains specialized pattern matchers for different type structures:
//! - Function type patterns
//! - Constructor type patterns
//! - Callable type patterns
//! - Object type patterns
//! - Object with index patterns
//! - Union type patterns
//! - Template literal patterns

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, FunctionShape, FunctionShapeId, ObjectShapeId, ParamInfo, TemplateSpan,
    TupleElement, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::utils;
use crate::{TypeSubstitution, instantiate_type};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn match_rest_infer_tuple(
        &self,
        source_params: &[ParamInfo],
        infer_ty: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let source_tuple_or_array = if source_params.len() == 1 && source_params[0].rest {
            source_params[0].type_id
        } else if source_params.iter().any(|param| param.rest) {
            return false;
        } else {
            let tuple_elems: Vec<TupleElement> = source_params
                .iter()
                .map(|p| TupleElement {
                    type_id: p.type_id,
                    name: p.name,
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.interner().tuple(tuple_elems)
        };
        let mut local_visited = FxHashSet::default();
        self.match_infer_pattern(
            source_tuple_or_array,
            infer_ty,
            bindings,
            &mut local_visited,
            checker,
        )
    }

    fn match_signature_params_for_infer(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let trailing_rest_param = pattern_params.last().filter(|param| param.rest);
        let fixed_param_count = if trailing_rest_param.is_some() {
            pattern_params.len().saturating_sub(1)
        } else {
            pattern_params.len()
        };

        if source_params.len() < fixed_param_count {
            return false;
        }

        let mut local_visited = FxHashSet::default();
        for (source_param, pattern_param) in source_params
            .iter()
            .take(fixed_param_count)
            .zip(pattern_params.iter().take(fixed_param_count))
        {
            let source_param_type = if source_param.optional {
                crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
            } else {
                source_param.type_id
            };
            if !self.match_infer_pattern(
                source_param_type,
                pattern_param.type_id,
                bindings,
                &mut local_visited,
                checker,
            ) {
                return false;
            }
        }

        if let Some(rest_param) = trailing_rest_param {
            let remaining_params = &source_params[fixed_param_count..];
            if self.type_contains_infer(rest_param.type_id) {
                if !self.match_rest_infer_tuple(
                    remaining_params,
                    rest_param.type_id,
                    bindings,
                    checker,
                ) {
                    return false;
                }
            } else {
                let mut local_visited = FxHashSet::default();
                for source_param in remaining_params {
                    let source_param_type = if source_param.optional {
                        crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
                    } else {
                        source_param.type_id
                    };
                    if !self.match_infer_pattern(
                        source_param_type,
                        rest_param.type_id,
                        bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                }
            }
        }

        true
    }

    pub(crate) fn match_infer_function_pattern(
        &self,
        source: TypeId,
        pattern_fn_id: FunctionShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        _visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_fn = self.interner().function_shape(pattern_fn_id);
        let has_param_infer = pattern_fn
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_fn.return_type);
        let has_single_rest_infer = pattern_fn.params.len() == 1
            && pattern_fn.params[0].rest
            && self.type_contains_infer(pattern_fn.params[0].type_id);

        if pattern_fn.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if has_single_rest_infer {
                    if !self.match_rest_infer_tuple(
                        source_params,
                        pattern_fn.params[0].type_id,
                        bindings,
                        checker,
                    ) {
                        return false;
                    }
                } else if !self.match_signature_params_for_infer(
                    source_params,
                    &pattern_fn.params,
                    bindings,
                    checker,
                ) {
                    return false;
                }
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For infer pattern matching, once parameters and return type match successfully,
                // the pattern is considered successful. The final subtype check is too strict
                // because of function parameter contravariance (e.g., any vs concrete type).
                // We've already matched the signature components above, which is sufficient.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Function)) => {
                    // Function intrinsic is structurally (...args: any[]) => any
                    let function_params = vec![crate::types::ParamInfo {
                        name: None,
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    }];
                    match_params_and_return(source, &function_params, TypeId::ANY, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_params_and_return(
                        source,
                        &source_fn.params,
                        source_fn.return_type,
                        bindings,
                    )
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    // Match against the last call signature (TypeScript behavior)
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.is_empty() {
                        return false;
                    }
                    // Use the last call signature (TypeScript's behavior for overloads)
                    // Safe to use last() here as we've verified the vector is not empty
                    let source_sig = match source_shape.call_signatures.last() {
                        Some(sig) => sig,
                        None => return false,
                    };
                    match_params_and_return(
                        source,
                        &source_sig.params,
                        source_sig.return_type,
                        bindings,
                    )
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if !match_params_and_return(
                                    member,
                                    &source_fn.params,
                                    source_fn.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.is_empty() {
                                    return false;
                                }
                                // Safe to use last() here as we've verified the vector is not empty
                                let source_sig = match source_shape.call_signatures.last() {
                                    Some(sig) => sig,
                                    None => return false,
                                };
                                if !match_params_and_return(
                                    member,
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_)) => {
                    // Check if the Object is structurally the Function interface
                    // (lowered without call signatures due to cross-arena splitting)
                    if crate::type_queries::is_function_interface_structural(
                        self.interner(),
                        source,
                    ) {
                        let function_params = vec![crate::types::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }];
                        match_params_and_return(source, &function_params, TypeId::ANY, bindings)
                    } else {
                        false
                    }
                }
                _ => false,
            };
        }

        if pattern_fn.this_type.is_none() && has_param_infer && !has_return_infer {
            if pattern_fn.is_constructor {
                return self.match_infer_constructor_pattern(
                    source,
                    &pattern_fn,
                    bindings,
                    checker,
                );
            }

            let has_single_rest_infer = pattern_fn.params.len() == 1
                && pattern_fn.params[0].rest
                && self.type_contains_infer(pattern_fn.params[0].type_id);

            if has_single_rest_infer {
                let infer_ty = pattern_fn.params[0].type_id;
                let erase_type_params =
                    |source_type_params: &[TypeParamInfo]| -> Option<TypeSubstitution> {
                        if source_type_params.is_empty() {
                            return None;
                        }
                        let mut subst = TypeSubstitution::new();
                        for tp in source_type_params {
                            subst.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
                        }
                        Some(subst)
                    };
                let mut match_params_tuple = |source_params: &[ParamInfo],
                                              source_type_params: &[TypeParamInfo],
                                              bindings: &mut FxHashMap<Atom, TypeId>|
                 -> bool {
                    let mut local_visited = FxHashSet::default();
                    let erased_subst = erase_type_params(source_type_params);

                    if source_params.len() == 1 && source_params[0].rest {
                        let source_param = &source_params[0];
                        let source_param_type = if let Some(subst) = &erased_subst {
                            instantiate_type(self.interner(), source_param.type_id, subst)
                        } else {
                            source_param.type_id
                        };
                        let source_param_type = if source_param.optional {
                            self.interner().union2(source_param_type, TypeId::UNDEFINED)
                        } else {
                            source_param_type
                        };
                        return self.match_infer_pattern(
                            source_param_type,
                            infer_ty,
                            bindings,
                            &mut local_visited,
                            checker,
                        );
                    }

                    let tuple_elems: Vec<TupleElement> = source_params
                        .iter()
                        .map(|param| TupleElement {
                            type_id: if let Some(subst) = &erased_subst {
                                instantiate_type(self.interner(), param.type_id, subst)
                            } else {
                                param.type_id
                            },
                            name: param.name,
                            optional: param.optional,
                            rest: param.rest,
                        })
                        .collect();
                    let tuple_ty = self.interner().tuple(tuple_elems);
                    self.match_infer_pattern(
                        tuple_ty,
                        infer_ty,
                        bindings,
                        &mut local_visited,
                        checker,
                    )
                };

                return match self.interner().lookup(source) {
                    Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Function)) => {
                        // Function intrinsic is structurally (...args: any[]) => any
                        let function_params = vec![crate::types::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }];
                        match_params_tuple(&function_params, &[], bindings)
                    }
                    Some(TypeData::Function(source_fn_id)) => {
                        let source_fn = self.interner().function_shape(source_fn_id);
                        match_params_tuple(&source_fn.params, &source_fn.type_params, bindings)
                    }
                    Some(TypeData::Callable(source_shape_id)) => {
                        let source_shape = self.interner().callable_shape(source_shape_id);
                        if source_shape.call_signatures.is_empty() {
                            return false;
                        }
                        let source_sig = source_shape
                            .call_signatures
                            .last()
                            .expect("call_signatures checked non-empty above");
                        match_params_tuple(&source_sig.params, &source_sig.type_params, bindings)
                    }
                    Some(TypeData::Union(members)) => {
                        let members = self.interner().type_list(members);
                        let mut combined = FxHashMap::default();
                        for &member in members.iter() {
                            let mut member_bindings = FxHashMap::default();
                            match self.interner().lookup(member) {
                                Some(TypeData::Function(source_fn_id)) => {
                                    let source_fn = self.interner().function_shape(source_fn_id);
                                    if !match_params_tuple(
                                        &source_fn.params,
                                        &source_fn.type_params,
                                        &mut member_bindings,
                                    ) {
                                        return false;
                                    }
                                }
                                Some(TypeData::Callable(source_shape_id)) => {
                                    let source_shape =
                                        self.interner().callable_shape(source_shape_id);
                                    if source_shape.call_signatures.is_empty() {
                                        return false;
                                    }
                                    let source_sig = source_shape
                                        .call_signatures
                                        .last()
                                        .expect("call_signatures checked non-empty above");
                                    if !match_params_tuple(
                                        &source_sig.params,
                                        &source_sig.type_params,
                                        &mut member_bindings,
                                    ) {
                                        return false;
                                    }
                                }
                                _ => return false,
                            }
                            for (name, ty) in member_bindings {
                                combined
                                    .entry(name)
                                    .and_modify(|existing| {
                                        *existing = self.interner().union2(*existing, ty);
                                    })
                                    .or_insert(ty);
                            }
                        }
                        bindings.extend(combined);
                        true
                    }
                    Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_)) => {
                        if crate::type_queries::is_function_interface_structural(
                            self.interner(),
                            source,
                        ) {
                            let function_params = vec![crate::types::ParamInfo {
                                name: None,
                                type_id: TypeId::ANY,
                                optional: false,
                                rest: true,
                            }];
                            match_params_tuple(&function_params, &[], bindings)
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
            }

            // Regular function parameter inference
            let mut match_function_params = |_source_type: TypeId,
                                             source_fn_id: FunctionShapeId,
                                             bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let source_fn = self.interner().function_shape(source_fn_id);
                if has_single_rest_infer {
                    return self.match_rest_infer_tuple(
                        &source_fn.params,
                        pattern_fn.params[0].type_id,
                        bindings,
                        checker,
                    );
                }
                self.match_signature_params_for_infer(
                    &source_fn.params,
                    &pattern_fn.params,
                    bindings,
                    checker,
                )
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    match_function_params(source, source_fn_id, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    // Match against the last call signature (TypeScript behavior for overloads)
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.is_empty() {
                        return false;
                    }
                    let source_sig = source_shape
                        .call_signatures
                        .last()
                        .expect("call_signatures checked non-empty above");
                    if has_single_rest_infer {
                        return self.match_rest_infer_tuple(
                            &source_sig.params,
                            pattern_fn.params[0].type_id,
                            bindings,
                            checker,
                        );
                    }
                    self.match_signature_params_for_infer(
                        &source_sig.params,
                        &pattern_fn.params,
                        bindings,
                        checker,
                    )
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeData::Function(source_fn_id)) = self.interner().lookup(member)
                        else {
                            return false;
                        };
                        let mut member_bindings = FxHashMap::default();
                        if !match_function_params(member, source_fn_id, &mut member_bindings) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }
        if pattern_fn.this_type.is_none() && !has_param_infer && has_return_infer {
            let mut match_return = |_source_type: TypeId,
                                    source_return: TypeId,
                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For return-only infer patterns, the return type match is sufficient.
                // Skipping the final subtype check avoids issues with contravariance.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_return(source, source_fn.return_type, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    // Match against the last call signature (TypeScript behavior)
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.is_empty() {
                        return false;
                    }
                    // Safe to use last() here as we've verified the vector is not empty
                    let source_sig = match source_shape.call_signatures.last() {
                        Some(sig) => sig,
                        None => return false,
                    };
                    match_return(source, source_sig.return_type, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if !match_return(
                                    member,
                                    source_fn.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.is_empty() {
                                    return false;
                                }
                                // Safe to use last() here as we've verified the vector is not empty
                                let source_sig = match source_shape.call_signatures.last() {
                                    Some(sig) => sig,
                                    None => return false,
                                };
                                if !match_return(
                                    member,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        let Some(pattern_this) = pattern_fn.this_type else {
            return checker.is_subtype_of(source, pattern);
        };
        if !self.type_contains_infer(pattern_this) {
            return checker.is_subtype_of(source, pattern);
        }

        if has_param_infer || has_return_infer {
            return false;
        }

        let mut match_function_this = |_source_type: TypeId,
                                       source_fn_id: FunctionShapeId,
                                       bindings: &mut FxHashMap<Atom, TypeId>|
         -> bool {
            let source_fn = self.interner().function_shape(source_fn_id);
            // Use Unknown instead of Any for stricter type checking
            // When this parameter type is not specified, use Unknown
            let source_this = source_fn.this_type.unwrap_or(TypeId::UNKNOWN);
            let mut local_visited = FxHashSet::default();
            if !self.match_infer_pattern(
                source_this,
                pattern_this,
                bindings,
                &mut local_visited,
                checker,
            ) {
                return false;
            }
            // For this-type infer patterns, the this type match is sufficient.
            // Skipping the final subtype check avoids contravariance issues.
            true
        };

        match self.interner().lookup(source) {
            Some(TypeData::Function(source_fn_id)) => {
                match_function_this(source, source_fn_id, bindings)
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let Some(TypeData::Function(source_fn_id)) = self.interner().lookup(member)
                    else {
                        return false;
                    };
                    let mut member_bindings = FxHashMap::default();
                    if !match_function_this(member, source_fn_id, &mut member_bindings) {
                        return false;
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching constructor function patterns.
    pub(crate) fn match_infer_constructor_pattern(
        &self,
        source: TypeId,
        pattern_fn: &FunctionShape,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // Check if pattern has a single rest parameter with infer type
        // e.g., new (...args: infer P) => any
        let has_single_rest_infer = pattern_fn.params.len() == 1
            && pattern_fn.params[0].rest
            && self.type_contains_infer(pattern_fn.params[0].type_id);

        if has_single_rest_infer {
            let infer_ty = pattern_fn.params[0].type_id;
            let mut match_construct_params_tuple = |source_params: &[ParamInfo],
                                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                // Build a tuple type from all source parameters
                let tuple_elems: Vec<TupleElement> = source_params
                    .iter()
                    .map(|p| TupleElement {
                        type_id: p.type_id,
                        name: p.name,
                        optional: p.optional,
                        rest: false,
                    })
                    .collect();
                let tuple_ty = self.interner().tuple(tuple_elems);

                // Match the tuple against the infer type
                let mut local_visited = FxHashSet::default();
                self.match_infer_pattern(tuple_ty, infer_ty, bindings, &mut local_visited, checker)
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if !source_fn.is_constructor {
                        return false;
                    }
                    match_construct_params_tuple(&source_fn.params, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.construct_signatures.is_empty() {
                        return false;
                    }
                    let source_sig = &source_shape.construct_signatures[0];
                    match_construct_params_tuple(&source_sig.params, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if !source_fn.is_constructor
                                    || !match_construct_params_tuple(
                                        &source_fn.params,
                                        &mut member_bindings,
                                    )
                                {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.construct_signatures.is_empty() {
                                    return false;
                                }
                                let source_sig = &source_shape.construct_signatures[0];
                                if !match_construct_params_tuple(
                                    &source_sig.params,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        // General case: match parameters individually
        let mut match_construct_params =
            |source_params: &[ParamInfo], bindings: &mut FxHashMap<Atom, TypeId>| -> bool {
                let mut local_visited = FxHashSet::default();
                self.match_signature_params(
                    source_params,
                    &pattern_fn.params,
                    bindings,
                    &mut local_visited,
                    checker,
                )
            };

        match self.interner().lookup(source) {
            Some(TypeData::Function(source_fn_id)) => {
                let source_fn = self.interner().function_shape(source_fn_id);
                if !source_fn.is_constructor {
                    return false;
                }
                match_construct_params(&source_fn.params, bindings)
            }
            Some(TypeData::Callable(source_shape_id)) => {
                let source_shape = self.interner().callable_shape(source_shape_id);
                if source_shape.construct_signatures.is_empty() {
                    return false;
                }
                let source_sig = &source_shape.construct_signatures[0];
                match_construct_params(&source_sig.params, bindings)
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    match self.interner().lookup(member) {
                        Some(TypeData::Function(source_fn_id)) => {
                            let source_fn = self.interner().function_shape(source_fn_id);
                            if !source_fn.is_constructor
                                || !match_construct_params(&source_fn.params, &mut member_bindings)
                            {
                                return false;
                            }
                        }
                        Some(TypeData::Callable(source_shape_id)) => {
                            let source_shape = self.interner().callable_shape(source_shape_id);
                            if source_shape.construct_signatures.is_empty() {
                                return false;
                            }
                            let source_sig = &source_shape.construct_signatures[0];
                            if !match_construct_params(&source_sig.params, &mut member_bindings) {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching callable type patterns.
    pub(crate) fn match_infer_callable_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        _visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);

        // Determine which signature to use: call or construct.
        // Pattern `new (...) => infer P` has construct_signatures, not call_signatures.
        let is_construct_pattern = pattern_shape.call_signatures.is_empty()
            && pattern_shape.construct_signatures.len() == 1
            && pattern_shape.properties.is_empty();
        let is_call_pattern = pattern_shape.construct_signatures.is_empty()
            && pattern_shape.call_signatures.len() == 1
            && pattern_shape.properties.is_empty();

        if !is_call_pattern && !is_construct_pattern {
            return checker.is_subtype_of(source, pattern);
        }
        let pattern_sig = if is_construct_pattern {
            &pattern_shape.construct_signatures[0]
        } else {
            &pattern_shape.call_signatures[0]
        };
        let has_param_infer = pattern_sig
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_sig.return_type);
        let has_single_rest_infer = pattern_sig.params.len() == 1
            && pattern_sig.params[0].rest
            && self.type_contains_infer(pattern_sig.params[0].type_id);
        if pattern_sig.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if has_single_rest_infer {
                    if !self.match_rest_infer_tuple(
                        source_params,
                        pattern_sig.params[0].type_id,
                        bindings,
                        checker,
                    ) {
                        return false;
                    }
                } else if !self.match_signature_params_for_infer(
                    source_params,
                    &pattern_sig.params,
                    bindings,
                    checker,
                ) {
                    return false;
                }
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For infer pattern matching, once parameters and return type match successfully,
                // the pattern is considered successful. Skipping the final subtype check avoids
                // contravariance issues.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    match_params_and_return(
                        source,
                        &source_sig.params,
                        source_sig.return_type,
                        bindings,
                    )
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    // For construct patterns, only match constructor Functions
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    match_params_and_return(
                        source,
                        &source_fn.params,
                        source_fn.return_type,
                        bindings,
                    )
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                if !match_params_and_return(
                                    member,
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                if !match_params_and_return(
                                    member,
                                    &source_fn.params,
                                    source_fn.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }
        if pattern_sig.this_type.is_none() && has_param_infer && !has_return_infer {
            let mut match_params =
                |source_params: &[ParamInfo], bindings: &mut FxHashMap<Atom, TypeId>| -> bool {
                    if has_single_rest_infer {
                        return self.match_rest_infer_tuple(
                            source_params,
                            pattern_sig.params[0].type_id,
                            bindings,
                            checker,
                        );
                    }
                    // Match params and infer types. Skip subtype check since pattern matching
                    // success implies compatibility. The subtype check can fail for optional
                    // params due to contravariance issues with undefined.
                    self.match_signature_params_for_infer(
                        source_params,
                        &pattern_sig.params,
                        bindings,
                        checker,
                    )
                };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    match_params(&source_sig.params, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    match_params(&source_fn.params, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                if !match_params(&source_sig.params, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                if !match_params(&source_fn.params, &mut member_bindings) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        if pattern_sig.this_type.is_none() && !has_param_infer && has_return_infer {
            let mut match_return = |_source_type: TypeId,
                                    source_return: TypeId,
                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For return-only infer patterns, the return type match is sufficient.
                // Skipping the final subtype check avoids contravariance issues.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    match_return(source, source_sig.return_type, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    match_return(source, source_fn.return_type, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                if !match_return(
                                    member,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                if !match_return(
                                    member,
                                    source_fn.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        checker.is_subtype_of(source, pattern)
    }

    /// Helper for matching object type patterns.
    pub(crate) fn match_infer_object_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        match self.interner().lookup(source) {
            Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) => {
                let source_shape = self.interner().object_shape(source_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = source_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Callable(callable_shape_id)) => {
                // Callable types (class constructors) have properties (static members)
                // that can match object patterns with infer. For example:
                // `typeof MyClass extends { defaultProps: infer D }` should match
                // when MyClass has a static `defaultProps` property.
                let callable_shape = self.interner().callable_shape(callable_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = callable_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Intersection(members)) => {
                let members = self.interner().type_list(members);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let mut merged_type = None;
                    for &member in members.iter() {
                        let found_type =
                            self.find_property_type_in_structural(member, pattern_prop.name);
                        if found_type.is_none() && !pattern_prop.optional {
                            // Non-optional pattern prop not found in this intersection
                            // member — if the member isn't Object/Callable, fail.
                            if !matches!(
                                self.interner().lookup(member),
                                Some(
                                    TypeData::Object(_)
                                        | TypeData::ObjectWithIndex(_)
                                        | TypeData::Callable(_)
                                )
                            ) {
                                return false;
                            }
                        }
                        if let Some(source_type) = found_type {
                            merged_type = Some(match merged_type {
                                Some(existing) => {
                                    self.interner().intersection2(existing, source_type)
                                }
                                None => source_type,
                            });
                        }
                    }

                    let Some(source_type) = merged_type else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };

                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    let mut local_visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Find a named property's type in a structural type (Object, ObjectWithIndex, or Callable).
    /// Returns `Some(type_id)` if the property is found, respecting optional property unwrapping.
    fn find_property_type_in_structural(&self, type_id: TypeId, prop_name: Atom) -> Option<TypeId> {
        match self.interner().lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| self.optional_property_type(p))
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = self.interner().callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| self.optional_property_type(p))
            }
            _ => None,
        }
    }

    /// Helper for matching object with index type patterns.
    pub(crate) fn match_infer_object_with_index_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        match self.interner().lookup(source) {
            Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) => {
                let source_shape = self.interner().object_shape(source_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = source_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }

                if let Some(pattern_index) = &pattern_shape.string_index {
                    if let Some(source_index) = &source_shape.string_index {
                        if !self.match_infer_pattern(
                            source_index.key_type,
                            pattern_index.key_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                        if !self.match_infer_pattern(
                            source_index.value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    } else {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::STRING,
                            pattern_index.key_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let values: Vec<TypeId> = source_shape
                            .properties
                            .iter()
                            .map(|prop| self.optional_property_type(prop))
                            .collect();
                        let value_type = if values.is_empty() {
                            TypeId::NEVER
                        } else if values.len() == 1 {
                            values[0]
                        } else {
                            self.interner().union(values)
                        };
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                }

                if let Some(pattern_index) = &pattern_shape.number_index {
                    if let Some(source_index) = &source_shape.number_index {
                        if !self.match_infer_pattern(
                            source_index.key_type,
                            pattern_index.key_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                        if !self.match_infer_pattern(
                            source_index.value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    } else {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::NUMBER,
                            pattern_index.key_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let values: Vec<TypeId> = source_shape
                            .properties
                            .iter()
                            .filter(|prop| {
                                utils::is_numeric_property_name(self.interner(), prop.name)
                            })
                            .map(|prop| self.optional_property_type(prop))
                            .collect();
                        let value_type = if values.is_empty() {
                            TypeId::NEVER
                        } else if values.len() == 1 {
                            values[0]
                        } else {
                            self.interner().union(values)
                        };
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                }

                true
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    let mut local_visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching union type patterns containing infer.
    pub(crate) fn match_infer_union_pattern(
        &self,
        source: TypeId,
        pattern_members: TypeListId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_members = self.interner().type_list(pattern_members);

        // Find infer members and non-infer members in the pattern
        let mut infer_members: Vec<(Atom, Option<TypeId>)> = Vec::new();
        let mut non_infer_pattern_members: Vec<TypeId> = Vec::new();

        for &pattern_member in pattern_members.iter() {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(pattern_member) {
                infer_members.push((info.name, info.constraint));
            } else {
                non_infer_pattern_members.push(pattern_member);
            }
        }

        // If no infer members, just do subtype check
        if infer_members.is_empty() {
            return checker.is_subtype_of(source, pattern);
        }

        // Currently only handle single infer in union pattern
        if infer_members.len() != 1 {
            return checker.is_subtype_of(source, pattern);
        }

        let (infer_name, infer_constraint) = infer_members[0];

        // Handle both union and non-union sources
        match self.interner().lookup(source) {
            Some(TypeData::Union(source_members)) => {
                let source_members = self.interner().type_list(source_members);

                // Find source members that DON'T match non-infer pattern members
                let mut remaining_source_members: Vec<TypeId> = Vec::new();

                for &source_member in source_members.iter() {
                    let mut matched = false;
                    for &non_infer in &non_infer_pattern_members {
                        if checker.is_subtype_of(source_member, non_infer)
                            && checker.is_subtype_of(non_infer, source_member)
                        {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        remaining_source_members.push(source_member);
                    }
                }

                // Bind infer to the remaining source members
                let inferred_type = if remaining_source_members.is_empty() {
                    TypeId::NEVER
                } else if remaining_source_members.len() == 1 {
                    remaining_source_members[0]
                } else {
                    self.interner().union(remaining_source_members)
                };

                self.bind_infer(
                    &TypeParamInfo {
                        is_const: false,
                        name: infer_name,
                        constraint: infer_constraint,
                        default: None,
                    },
                    inferred_type,
                    bindings,
                    checker,
                )
            }
            _ => {
                // Source is not a union - check if source matches any non-infer pattern member
                for &non_infer in &non_infer_pattern_members {
                    if checker.is_subtype_of(source, non_infer)
                        && checker.is_subtype_of(non_infer, source)
                    {
                        // Source is exactly a non-infer member, so infer gets never
                        return self.bind_infer(
                            &TypeParamInfo {
                                is_const: false,
                                name: infer_name,
                                constraint: infer_constraint,
                                default: None,
                            },
                            TypeId::NEVER,
                            bindings,
                            checker,
                        );
                    }
                }
                // Source doesn't match non-infer members, so infer = source
                self.bind_infer(
                    &TypeParamInfo {
                        is_const: false,
                        name: infer_name,
                        constraint: infer_constraint,
                        default: None,
                    },
                    source,
                    bindings,
                    checker,
                )
            }
        }
    }

    /// Match a template literal string against a pattern.
    pub(crate) fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut pos = 0;
        let mut index = 0;

        while index < pattern.len() {
            match pattern[index] {
                TemplateSpan::Text(text) => {
                    let text_value = self.interner().resolve_atom_ref(text);
                    let text_value = text_value.as_ref();
                    if !source[pos..].starts_with(text_value) {
                        return false;
                    }
                    pos += text_value.len();
                    index += 1;
                }
                TemplateSpan::Type(type_id) => {
                    let next_text = pattern[index + 1..].iter().find_map(|span| match span {
                        TemplateSpan::Text(text) => Some(*text),
                        TemplateSpan::Type(_) => None,
                    });
                    // Check if the next span is another Type (no text separator).
                    // In tsc, consecutive infer types like `${infer C}${infer R}`
                    // require the first infer to capture exactly 1 character.
                    // Without this, `""` would match both infers with empty strings,
                    // causing infinite recursion in tail-recursive conditional types
                    // like `type GetChars<S> = S extends `${infer C}${infer R}` ? ... : ...`.
                    let next_is_type = pattern
                        .get(index + 1)
                        .is_some_and(|s| matches!(s, TemplateSpan::Type(_)));
                    let end = if let Some(next_text) = next_text {
                        let next_value = self.interner().resolve_atom_ref(next_text);
                        // When there are no more Type (infer) spans after the next text
                        // separator, the text must match at the END of the remaining string.
                        // Use rfind (last occurrence) so the infer captures greedily.
                        // Example: `${infer R} ` matching "hello  " → R = "hello " (rfind)
                        //
                        // When more Type spans follow, use find (first occurrence) so each
                        // infer captures minimally, leaving content for later infers.
                        // Example: `${infer A}.${infer B}` matching "a.b.c" → A = "a" (find)
                        let has_more_types_after_separator = pattern[index + 1..]
                            .iter()
                            .skip_while(|s| !matches!(s, TemplateSpan::Text(_)))
                            .skip(1) // skip the text separator itself
                            .any(|s| matches!(s, TemplateSpan::Type(_)));
                        let search_fn = if has_more_types_after_separator {
                            str::find
                        } else {
                            str::rfind
                        };
                        match search_fn(&source[pos..], next_value.as_ref()) {
                            Some(offset) => pos + offset,
                            None => return false,
                        }
                    } else if next_is_type {
                        // Consecutive infer types: capture exactly 1 character.
                        // This matches tsc behavior where `${infer C}${infer R}`
                        // splits "AB" as C="A", R="B" and fails on "".
                        if pos >= source.len() {
                            return false; // Not enough characters for this infer
                        }
                        // Find the next char boundary (for UTF-8 safety)

                        source[pos..]
                            .char_indices()
                            .nth(1)
                            .map_or(source.len(), |(idx, _)| pos + idx)
                    } else {
                        source.len()
                    };

                    let captured = &source[pos..end];
                    pos = end;
                    let captured_type = self.interner().literal_string(captured);

                    if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                        if !self.bind_infer(&info, captured_type, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(captured_type, type_id) {
                        return false;
                    }
                    index += 1;
                }
            }
        }

        pos == source.len()
    }

    /// Match template literal spans against a pattern.
    pub(crate) fn match_template_literal_spans(
        &self,
        source: TypeId,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans.len() == 1
            && let TemplateSpan::Type(type_id) = pattern_spans[0]
        {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                let inferred = if source_spans
                    .iter()
                    .all(|span| matches!(span, TemplateSpan::Type(_)))
                {
                    TypeId::STRING
                } else {
                    source
                };
                return self.bind_infer(&info, inferred, bindings, checker);
            }
            return checker.is_subtype_of(source, type_id);
        }

        if source_spans.len() != pattern_spans.len() {
            return false;
        }

        for (source_span, pattern_span) in source_spans.iter().zip(pattern_spans.iter()) {
            match pattern_span {
                TemplateSpan::Text(text) => match source_span {
                    TemplateSpan::Text(source_text) if source_text == text => {}
                    _ => return false,
                },
                TemplateSpan::Type(type_id) => {
                    let inferred = match source_span {
                        TemplateSpan::Text(text) => {
                            let text_value = self.interner().resolve_atom_ref(*text);
                            self.interner().literal_string(text_value.as_ref())
                        }
                        TemplateSpan::Type(source_type) => *source_type,
                    };
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(*type_id) {
                        if !self.bind_infer(&info, inferred, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(inferred, *type_id) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Match a string type against a template literal pattern.
    pub(crate) fn match_template_literal_string_type(
        &self,
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans
            .iter()
            .any(|span| matches!(span, TemplateSpan::Text(_)))
        {
            return false;
        }

        for span in pattern_spans {
            if let TemplateSpan::Type(type_id) = span {
                if let Some(TypeData::Infer(info)) = self.interner().lookup(*type_id) {
                    if !self.bind_infer(&info, TypeId::STRING, bindings, checker) {
                        return false;
                    }
                } else if !checker.is_subtype_of(TypeId::STRING, *type_id) {
                    return false;
                }
            }
        }

        true
    }
}
