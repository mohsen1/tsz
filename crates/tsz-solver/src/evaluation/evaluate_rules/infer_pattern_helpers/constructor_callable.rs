use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
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
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_sig.params,
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    // For construct patterns, only match constructor Functions
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_fn.params,
                        source_fn.return_type,
                        &source_fn.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
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
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
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
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_fn.params,
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
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
}
