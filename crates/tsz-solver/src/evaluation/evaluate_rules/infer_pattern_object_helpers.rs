//! Infer pattern matching helpers for object, union, and template-literal
//! patterns.
//!
//! Split out of `infer_pattern_helpers.rs` to keep both files under the
//! file-size ceiling (section 19). These remain methods on `TypeEvaluator`
//! in the same crate; helpers shared across the split are `pub(crate)`.

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectShapeId, PropertyInfo, TemplateSpan, TypeData, TypeId,
    TypeListId, TypeParamInfo,
};
use crate::utils;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
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

    /// Match a template literal string against a pattern.
    pub(crate) fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        self.match_template_literal_string_from(source, pattern, 0, 0, bindings, checker)
    }

    fn match_template_segment_prefix(
        &self,
        source: &str,
        pos: usize,
        type_id: TypeId,
    ) -> Option<usize> {
        match self.interner().lookup(type_id)? {
            TypeData::Literal(LiteralValue::String(atom)) => {
                let text = self.interner().resolve_atom(atom);
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            TypeData::Union(list_id) => self
                .interner()
                .type_list(list_id)
                .iter()
                .find_map(|member| self.match_template_segment_prefix(source, pos, *member)),
            TypeData::TemplateLiteral(template_id) => {
                let spans = self.interner().template_list(template_id);
                let mut text = String::new();
                for span in spans.iter() {
                    let TemplateSpan::Text(atom) = span else {
                        return None;
                    };
                    text.push_str(&self.interner().resolve_atom(*atom));
                }
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            _ => None,
        }
    }

    fn is_template_infer_span(&self, span: Option<&TemplateSpan>) -> bool {
        span.is_some_and(|span| {
            matches!(span, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_))))
        })
    }

    fn next_char_end(source: &str, pos: usize) -> Option<usize> {
        if pos >= source.len() {
            return None;
        }
        Some(
            source[pos..]
                .char_indices()
                .nth(1)
                .map_or(source.len(), |(idx, _)| pos + idx),
        )
    }

    fn candidate_template_capture_ends(
        &self,
        source: &str,
        pos: usize,
        pattern: &[TemplateSpan],
        index: usize,
    ) -> Vec<usize> {
        if index + 1 >= pattern.len() {
            return vec![source.len()];
        }

        if self.is_template_infer_span(pattern.get(index))
            && matches!(
                pattern.get(index + 1),
                Some(TemplateSpan::Type(
                    TypeId::STRING | TypeId::ANY | TypeId::UNKNOWN
                ))
            )
        {
            if self.is_template_infer_span(pattern.get(index + 2)) {
                return Self::next_char_end(source, pos).into_iter().collect();
            }

            return Self::next_char_end(source, pos)
                .or(Some(pos))
                .into_iter()
                .collect();
        }

        if pattern
            .get(index + 1)
            .is_some_and(|s| matches!(s, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_)))))
        {
            return Self::next_char_end(source, pos).into_iter().collect();
        }

        if let Some(next_text) = pattern[index + 1..].iter().find_map(|span| match span {
            TemplateSpan::Text(text) => Some(*text),
            TemplateSpan::Type(_) => None,
        }) {
            let next_value = self.interner().resolve_atom_ref(next_text);
            let remaining = &source[pos..];
            return remaining
                .match_indices(next_value.as_ref())
                .map(|(offset, _)| pos + offset)
                .collect();
        }

        source[pos..]
            .char_indices()
            .map(|(offset, _)| pos + offset)
            .chain(std::iter::once(source.len()))
            .collect()
    }

    /// Match an intrinsic-typed span at position `pos` in the infer-pattern path.
    ///
    /// Returns `Some(true/false)` when the span is a recognized intrinsic kind
    /// (number, bigint, boolean, null, undefined) and dispatches length-aware
    /// matching for it.  Returns `None` for wildcard intrinsics (string/any/
    /// unknown) so the caller falls through to generic handling.
    fn match_intrinsic_span_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        type_id: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        use crate::relations::subtype::rules::literals::{
            find_integer_length, find_number_length, is_valid_number,
        };

        let remaining = &source[pos..];

        match self.interner().lookup(type_id)? {
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::Number => {
                    let num_len = find_number_length(remaining);
                    if num_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid number first — matches tsc's non-greedy
                    // behaviour for ambiguous infer+number patterns.
                    for len in 1..=num_len {
                        if is_valid_number(&remaining[..len])
                            && self.match_template_literal_string_from(
                                source,
                                pattern,
                                pos + len,
                                index + 1,
                                bindings,
                                checker,
                            )
                        {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Bigint => {
                    let int_len = find_integer_length(remaining);
                    if int_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid integer first — consistent with tsc.
                    for len in 1..=int_len {
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + len,
                            index + 1,
                            bindings,
                            checker,
                        ) {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Boolean => {
                    if remaining.starts_with("true")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    if remaining.starts_with("false")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 5,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    Some(false)
                }
                IntrinsicKind::Null => {
                    if remaining.starts_with("null")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                IntrinsicKind::Undefined => {
                    if remaining.starts_with("undefined")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 9,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                // Wildcards and other intrinsics fall through to generic handling.
                _ => None,
            },
            _ => None,
        }
    }

    fn match_template_literal_string_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if index == pattern.len() {
            return pos == source.len();
        }

        match pattern[index] {
            TemplateSpan::Text(text) => {
                let text_value = self.interner().resolve_atom_ref(text);
                let text_value = text_value.as_ref();
                if !source[pos..].starts_with(text_value) {
                    return false;
                }
                self.match_template_literal_string_from(
                    source,
                    pattern,
                    pos + text_value.len(),
                    index + 1,
                    bindings,
                    checker,
                )
            }
            TemplateSpan::Type(type_id) => {
                if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                    for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                        let mut next_bindings = bindings.clone();
                        let captured = &source[pos..end];
                        if !self.bind_template_infer_capture(
                            &info,
                            captured,
                            &mut next_bindings,
                            checker,
                        ) {
                            continue;
                        }
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            &mut next_bindings,
                            checker,
                        ) {
                            *bindings = next_bindings;
                            return true;
                        }
                    }
                    return false;
                }

                if let Some(next_pos) = self.match_template_segment_prefix(source, pos, type_id) {
                    return self.match_template_literal_string_from(
                        source,
                        pattern,
                        next_pos,
                        index + 1,
                        bindings,
                        checker,
                    );
                }

                if let Some(result) = self.match_intrinsic_span_from(
                    source, pattern, pos, index, type_id, bindings, checker,
                ) {
                    return result;
                }

                for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                    let captured = &source[pos..end];
                    let captured_type = self.interner().literal_string(captured);
                    if self
                        .template_capture_for_constraint(captured, captured_type, type_id, checker)
                        .is_some()
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Capture value for a bare single-placeholder `` `${infer V}` `` pattern
    /// matched against a template-literal `source`.
    ///
    /// tsc captures the whole source type (`getStringLikeTypeForType` of the
    /// placeholder) rather than widening to `string`: inferring `` `${infer V}` ``
    /// from `` `${number}` `` yields `` `${number}` ``, not `string`.
    ///
    /// When the infer variable carries an `extends` constraint, this mirrors
    /// tsc's `getInferredType` fallback: if the captured template type isn't
    /// assignable to the constraint, fall back to the constraint itself, but
    /// only when the source is assignable to the constraint's string form
    /// (`` `${C}` ``) — i.e. when the conditional's post-inference `extends`
    /// re-check would still succeed. Otherwise the match fails and the
    /// conditional takes its false branch.
    fn single_placeholder_template_capture(
        &self,
        source: TypeId,
        info: &TypeParamInfo,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        let Some(constraint) = info.constraint else {
            return Some(source);
        };
        if checker.is_subtype_of(source, constraint) {
            return Some(source);
        }
        let constraint_string_form = self
            .interner()
            .template_literal(vec![TemplateSpan::Type(constraint)]);
        checker
            .is_subtype_of(source, constraint_string_form)
            .then_some(constraint)
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
                let Some(inferred) =
                    self.single_placeholder_template_capture(source, &info, checker)
                else {
                    return false;
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
}
