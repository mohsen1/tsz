//! Object, callable-property, and union infer-pattern matching helpers.

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, PropertyInfo, TypeData, TypeId,
    TypeListId, TypeParamInfo,
};
use crate::utils;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(crate) fn match_infer_callable_pattern_properties(
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
            let mut local_visited = FxHashSet::default();
            if !self.match_infer_pattern(
                source_type,
                pattern_prop.type_id,
                &mut local,
                &mut local_visited,
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
                    let mut alias_visited = visited.clone();
                    if self.match_infer_pattern(
                        alias,
                        pattern,
                        &mut alias_bindings,
                        &mut alias_visited,
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
            let mut key_visited = FxHashSet::default();
            if !self.match_infer_pattern(
                TypeId::NUMBER,
                pattern_index.key_type,
                bindings,
                &mut key_visited,
                checker,
            ) {
                return false;
            }
            let mut value_visited = FxHashSet::default();
            return self.match_infer_pattern(
                source_elem,
                pattern_index.value_type,
                bindings,
                &mut value_visited,
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
}
