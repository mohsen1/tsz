//! Callable and object infer pattern matching helpers.

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, ParamInfo, PropertyInfo, TypeData,
    TypeId,
};
use crate::utils;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(crate) fn match_infer_callable_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);

        if pattern_shape
            .properties
            .iter()
            .any(|prop| self.type_contains_infer(prop.type_id))
            && self.match_infer_callable_pattern_properties(
                source,
                pattern_shape_id,
                bindings,
                checker,
            )
        {
            return true;
        }

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
                    visited,
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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
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
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    visited,
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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
                        return false;
                    };
                    let erased_return = self.erase_return_type_for_infer(
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_return(source, erased_return, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    let erased_return = self
                        .erase_return_type_for_infer(source_fn.return_type, &source_fn.type_params);
                    match_return(source, erased_return, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
                                    return false;
                                };
                                let erased_return = self.erase_return_type_for_infer(
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_return(member, erased_return, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                let erased_return = self.erase_return_type_for_infer(
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_return(member, erased_return, &mut member_bindings) {
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

    fn match_infer_callable_pattern_properties(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);
        let Some(source_shape_id) = self.source_callable_shape_id(source) else {
            return false;
        };
        let source_shape = self.interner().callable_shape(source_shape_id);
        if pattern_shape.call_signatures.len() > source_shape.call_signatures.len()
            || pattern_shape.construct_signatures.len() > source_shape.construct_signatures.len()
        {
            return false;
        }

        for pattern_prop in &pattern_shape.properties {
            let source_prop = source_shape
                .properties
                .iter()
                .find(|prop| prop.name == pattern_prop.name);
            let Some(source_prop) = source_prop else {
                if pattern_prop.optional {
                    if self.type_contains_infer(pattern_prop.type_id) {
                        let mut visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::UNDEFINED,
                            pattern_prop.type_id,
                            bindings,
                            &mut visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                    continue;
                }
                return false;
            };

            if self.type_contains_infer(pattern_prop.type_id) {
                let mut visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_prop.type_id,
                    pattern_prop.type_id,
                    bindings,
                    &mut visited,
                    checker,
                ) {
                    return false;
                }
            } else if !checker.is_subtype_of(
                self.optional_property_type(source_prop),
                self.optional_property_type(pattern_prop),
            ) {
                return false;
            }
        }
        true
    }

    fn source_callable_shape_id(&self, source: TypeId) -> Option<CallableShapeId> {
        match self.interner().lookup(source) {
            Some(TypeData::Callable(shape_id)) => Some(shape_id),
            Some(TypeData::ReadonlyType(inner)) => self.source_callable_shape_id(inner),
            Some(TypeData::Intersection(members)) => self
                .interner()
                .type_list(members)
                .iter()
                .find_map(|&member| self.source_callable_shape_id(member)),
            _ => None,
        }
    }

    /// Match each pattern property against the corresponding source property,
    /// extracting infer bindings with variance-aware merging.
    ///
    /// Each property is matched against a fresh copy of the incoming bindings so
    /// that the order of properties does not affect the result, then its
    /// candidates are merged via [`Self::merge_infer_candidates`]. When the same
    /// `infer` name appears in both a covariant property slot and a contravariant
    /// one (e.g. `{ v: infer U; f: (x: infer U) => void }`), the candidates are
    /// intersected rather than failing the match through `bind_infer`'s
    /// equality requirement — matching tsc, which infers `string & number` here.
    fn match_infer_object_properties(
        &self,
        source_props: &[PropertyInfo],
        pattern_props: &[PropertyInfo],
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let contravariant_infers = self.collect_contravariant_infer_names(pattern);
        let base = bindings.clone();
        let mut merged = base.clone();
        for pattern_prop in pattern_props {
            let source_prop = source_props
                .iter()
                .find(|prop| prop.name == pattern_prop.name);
            let source_type = match source_prop {
                Some(source_prop) => {
                    if self.type_contains_infer(pattern_prop.type_id) {
                        source_prop.type_id
                    } else {
                        self.optional_property_type(source_prop)
                    }
                }
                None => {
                    if !pattern_prop.optional {
                        return false;
                    }
                    if !self.type_contains_infer(pattern_prop.type_id) {
                        continue;
                    }
                    TypeId::UNDEFINED
                }
            };
            let mut local = base.clone();
            if !self.match_infer_pattern(
                source_type,
                pattern_prop.type_id,
                &mut local,
                visited,
                checker,
            ) {
                return false;
            }
            self.merge_infer_candidates(&base, &mut merged, local, &contravariant_infers);
        }
        *bindings = merged;
        true
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
                let initial_binding_len = bindings.len();
                let source_shape = self.interner().object_shape(source_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                if !self.match_infer_object_properties(
                    &source_shape.properties,
                    &pattern_shape.properties,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ) {
                    return false;
                }
                if bindings.len() == initial_binding_len
                    && self.type_contains_infer(pattern)
                    && let Some(alias) = self.interner().get_display_alias(source)
                    && alias != source
                {
                    let mut alias_bindings = bindings.clone();
                    if self.match_infer_pattern(
                        alias,
                        pattern,
                        &mut alias_bindings,
                        visited,
                        checker,
                    ) && alias_bindings.len() > initial_binding_len
                    {
                        *bindings = alias_bindings;
                    }
                }
                true
            }
            Some(TypeData::Application(_)) => {
                let mut evaluator = TypeEvaluator::with_resolver(self.interner(), self.resolver());
                evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access());
                if let Some(query_db) = self.query_db() {
                    evaluator = evaluator.with_query_db(query_db);
                }
                let evaluated = evaluator.evaluate(source);
                if evaluated == source {
                    return false;
                }
                self.match_infer_object_pattern(
                    evaluated,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                )
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
                    let source_type = if self.type_contains_infer(pattern_prop.type_id) {
                        source_prop.type_id
                    } else {
                        self.optional_property_type(source_prop)
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
            Some(TypeData::Intersection(members)) => {
                let members = self.interner().type_list(members);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let mut merged_type = None;
                    for &member in members.iter() {
                        let found_type = self.find_property_type_in_structural(
                            member,
                            pattern_prop.name,
                            self.type_contains_infer(pattern_prop.type_id),
                        );
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
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        visited,
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
            Some(
                TypeData::Tuple(_)
                | TypeData::Array(_)
                | TypeData::ReadonlyType(_)
                | TypeData::Intrinsic(IntrinsicKind::String)
                | TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_),
            ) => {
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let Some(source_type) =
                        self.implicit_sequence_property_type(source, pattern_prop.name)
                    else {
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
            _ => false,
        }
    }

    /// Find a named property's type in a structural type (`Object`, `ObjectWithIndex`, or `Callable`).
    fn find_property_type_in_structural(
        &self,
        type_id: TypeId,
        prop_name: Atom,
        raw_if_infer: bool,
    ) -> Option<TypeId> {
        let evaluated = match self.interner().lookup(type_id) {
            Some(TypeData::Application(_)) | Some(TypeData::Mapped(_)) => {
                let mut evaluator = TypeEvaluator::with_resolver(self.interner(), self.resolver());
                evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access());
                if let Some(query_db) = self.query_db() {
                    evaluator = evaluator.with_query_db(query_db);
                }
                let evaluated = evaluator.evaluate(type_id);
                if evaluated == type_id {
                    type_id
                } else {
                    evaluated
                }
            }
            _ => type_id,
        };

        match self.interner().lookup(evaluated) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| {
                        if raw_if_infer {
                            p.type_id
                        } else {
                            self.optional_property_type(p)
                        }
                    })
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = self.interner().callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| {
                        if raw_if_infer {
                            p.type_id
                        } else {
                            self.optional_property_type(p)
                        }
                    })
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
        let pattern_shape = self.interner().object_shape(pattern_shape_id);
        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner(), source)
            && let Some(pattern_index) = &pattern_shape.number_index
        {
            if !self.match_infer_pattern(
                TypeId::NUMBER,
                pattern_index.key_type,
                bindings,
                visited,
                checker,
            ) {
                return false;
            }
            return self.match_infer_pattern(
                source_elem,
                pattern_index.value_type,
                bindings,
                visited,
                checker,
            );
        }

        match self.interner().lookup(source) {
            Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) => {
                let source_shape = self.interner().object_shape(source_shape_id);
                if !self.match_infer_object_properties(
                    &source_shape.properties,
                    &pattern_shape.properties,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ) {
                    return false;
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
                        if !self.match_infer_pattern(
                            TypeId::STRING,
                            pattern_index.key_type,
                            bindings,
                            visited,
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
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
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
                        if !self.match_infer_pattern(
                            TypeId::NUMBER,
                            pattern_index.key_type,
                            bindings,
                            visited,
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
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                }

                true
            }
            Some(TypeData::Application(_)) => {
                let mut evaluator = TypeEvaluator::with_resolver(self.interner(), self.resolver());
                evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access());
                if let Some(query_db) = self.query_db() {
                    evaluator = evaluator.with_query_db(query_db);
                }
                let evaluated = evaluator.evaluate(source);
                if evaluated == source {
                    return false;
                }
                self.match_infer_object_with_index_pattern(
                    evaluated,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                )
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        visited,
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
}
