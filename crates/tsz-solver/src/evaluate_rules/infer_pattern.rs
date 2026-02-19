//! Infer pattern matching for conditional types.
//!
//! Handles TypeScript's `infer` keyword in conditional types.
//! This module provides:
//! - Pattern matching for extracting types from complex type structures
//! - Binding inferred types to infer type parameters
//! - Substitution of infer bindings back into types
//!
//! Key functions:
//! - `match_infer_pattern`: Main entry point for pattern matching
//! - `substitute_infer`: Replace infer types with their bindings
//! - `bind_infer`: Bind a type to an infer parameter

use crate::application::ApplicationEvaluator;
use crate::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    IntrinsicKind, LiteralValue, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;
use super::infer_substitutor::InferSubstitutor;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Substitute infer bindings into a type.
    ///
    /// Replaces all `infer X` references with their bound values from the bindings map.
    pub(crate) fn substitute_infer(
        &self,
        type_id: TypeId,
        bindings: &FxHashMap<Atom, TypeId>,
    ) -> TypeId {
        if bindings.is_empty() {
            return type_id;
        }
        let mut substitutor = InferSubstitutor::new(self.interner(), bindings);
        substitutor.substitute(type_id)
    }

    /// Check if a type contains any `infer` type parameters.
    pub(crate) fn type_contains_infer(&self, type_id: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.type_contains_infer_inner(type_id, &mut visited)
    }

    fn type_contains_infer_inner(&self, type_id: TypeId, visited: &mut FxHashSet<TypeId>) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        let Some(key) = self.interner().lookup(type_id) else {
            return false;
        };

        match key {
            TypeData::Infer(_) => true,
            TypeData::Array(elem) => self.type_contains_infer_inner(elem, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                elements
                    .iter()
                    .any(|element| self.type_contains_infer_inner(element.type_id, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_infer_inner(member, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.string_index
                    && (self.type_contains_infer_inner(index.key_type, visited)
                        || self.type_contains_infer_inner(index.value_type, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.number_index
                    && (self.type_contains_infer_inner(index.key_type, visited)
                        || self.type_contains_infer_inner(index.value_type, visited))
                {
                    return true;
                }
                false
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner().function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                    || shape
                        .this_type
                        .is_some_and(|this_type| self.type_contains_infer_inner(this_type, visited))
                    || self.type_contains_infer_inner(shape.return_type, visited)
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                shape.call_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_infer_inner(this_type, visited)
                        })
                        || self.type_contains_infer_inner(sig.return_type, visited)
                }) || shape.construct_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_infer_inner(this_type, visited)
                        })
                        || self.type_contains_infer_inner(sig.return_type, visited)
                }) || shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeData::TypeParameter(info) => {
                info.constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || info
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
            }
            TypeData::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                self.type_contains_infer_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_infer_inner(arg, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner().conditional_type(cond_id);
                self.type_contains_infer_inner(cond.check_type, visited)
                    || self.type_contains_infer_inner(cond.extends_type, visited)
                    || self.type_contains_infer_inner(cond.true_type, visited)
                    || self.type_contains_infer_inner(cond.false_type, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner().mapped_type(mapped_id);
                mapped
                    .type_param
                    .constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || mapped
                        .type_param
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
                    || self.type_contains_infer_inner(mapped.constraint, visited)
                    || mapped
                        .name_type
                        .is_some_and(|name_type| self.type_contains_infer_inner(name_type, visited))
                    || self.type_contains_infer_inner(mapped.template, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_infer_inner(obj, visited)
                    || self.type_contains_infer_inner(idx, visited)
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.type_contains_infer_inner(inner, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner().template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_infer_inner(*inner, visited),
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_infer_inner(type_arg, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_infer_inner(member_type, visited)
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }

    /// Filter an inferred type by its constraint.
    ///
    /// Returns `Some(filtered_type)` if any part of the inferred type satisfies the constraint,
    /// or None if no part satisfies it.
    pub(crate) fn filter_inferred_by_constraint(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        if inferred == constraint {
            return Some(inferred);
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(inferred) {
            let members = self.interner().type_list(members);
            let mut filtered = Vec::new();
            for &member in members.iter() {
                if checker.is_subtype_of(member, constraint) {
                    filtered.push(member);
                }
            }
            return match filtered.len() {
                0 => None,
                1 => Some(filtered[0]),
                _ => Some(self.interner().union(filtered)),
            };
        }

        checker
            .is_subtype_of(inferred, constraint)
            .then_some(inferred)
    }

    /// Filter an inferred type by its constraint, returning undefined for non-matching parts.
    ///
    /// Similar to `filter_inferred_by_constraint`, but returns `undefined` instead of None
    /// when parts don't match the constraint.
    pub(crate) fn filter_inferred_by_constraint_or_undefined(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> TypeId {
        if inferred == constraint {
            return inferred;
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(inferred) {
            let members = self.interner().type_list(members);
            let mut filtered = Vec::new();
            let mut had_non_matching = false;
            for &member in members.iter() {
                if checker.is_subtype_of(member, constraint) {
                    filtered.push(member);
                } else {
                    had_non_matching = true;
                }
            }

            if had_non_matching {
                filtered.push(TypeId::UNDEFINED);
            }

            return match filtered.len() {
                0 => TypeId::UNDEFINED,
                1 => filtered[0],
                _ => self.interner().union(filtered),
            };
        }

        if checker.is_subtype_of(inferred, constraint) {
            inferred
        } else {
            TypeId::UNDEFINED
        }
    }

    /// Bind an inferred type to an infer parameter.
    ///
    /// Handles constraint checking and merging with existing bindings.
    pub(crate) fn bind_infer(
        &self,
        info: &TypeParamInfo,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut inferred = inferred;
        if let Some(constraint) = info.constraint {
            let Some(filtered) = self.filter_inferred_by_constraint(inferred, constraint, checker)
            else {
                return false;
            };
            inferred = filtered;
        }

        if let Some(existing) = bindings.get(&info.name) {
            return checker.is_subtype_of(inferred, *existing)
                && checker.is_subtype_of(*existing, inferred);
        }

        bindings.insert(info.name, inferred);
        true
    }

    /// Bind default values for all infer parameters in a pattern.
    ///
    /// Used when the source type doesn't provide a value for an infer parameter.
    pub(crate) fn bind_infer_defaults(
        &self,
        pattern: TypeId,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.bind_infer_defaults_inner(pattern, inferred, bindings, checker, &mut visited)
    }

    fn bind_infer_defaults_inner(
        &self,
        pattern: TypeId,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(pattern) {
            return true;
        }

        let Some(key) = self.interner().lookup(pattern) else {
            return true;
        };

        match key {
            TypeData::Infer(info) => self.bind_infer(&info, inferred, bindings, checker),
            TypeData::Array(elem) => {
                self.bind_infer_defaults_inner(elem, inferred, bindings, checker, visited)
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                for element in elements.iter() {
                    if !self.bind_infer_defaults_inner(
                        element.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if !self.bind_infer_defaults_inner(member, inferred, bindings, checker, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                if let Some(index) = &shape.string_index
                    && (!self.bind_infer_defaults_inner(
                        index.key_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) || !self.bind_infer_defaults_inner(
                        index.value_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ))
                {
                    return false;
                }
                if let Some(index) = &shape.number_index
                    && (!self.bind_infer_defaults_inner(
                        index.key_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) || !self.bind_infer_defaults_inner(
                        index.value_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ))
                {
                    return false;
                }
                true
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner().function_shape(shape_id);
                for param in &shape.params {
                    if !self.bind_infer_defaults_inner(
                        param.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                if let Some(this_type) = shape.this_type
                    && !self
                        .bind_infer_defaults_inner(this_type, inferred, bindings, checker, visited)
                {
                    return false;
                }
                self.bind_infer_defaults_inner(
                    shape.return_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        if !self.bind_infer_defaults_inner(
                            param.type_id,
                            inferred,
                            bindings,
                            checker,
                            visited,
                        ) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.bind_infer_defaults_inner(
                            this_type, inferred, bindings, checker, visited,
                        )
                    {
                        return false;
                    }
                    if !self.bind_infer_defaults_inner(
                        sig.return_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        if !self.bind_infer_defaults_inner(
                            param.type_id,
                            inferred,
                            bindings,
                            checker,
                            visited,
                        ) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.bind_infer_defaults_inner(
                            this_type, inferred, bindings, checker, visited,
                        )
                    {
                        return false;
                    }
                    if !self.bind_infer_defaults_inner(
                        sig.return_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                for prop in &shape.properties {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeData::TypeParameter(info) => {
                if let Some(constraint) = info.constraint
                    && !self
                        .bind_infer_defaults_inner(constraint, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if let Some(default) = info.default
                    && !self
                        .bind_infer_defaults_inner(default, inferred, bindings, checker, visited)
                {
                    return false;
                }
                true
            }
            TypeData::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                if !self.bind_infer_defaults_inner(app.base, inferred, bindings, checker, visited) {
                    return false;
                }
                for &arg in &app.args {
                    if !self.bind_infer_defaults_inner(arg, inferred, bindings, checker, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner().conditional_type(cond_id);
                self.bind_infer_defaults_inner(
                    cond.check_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.extends_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.true_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.false_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner().mapped_type(mapped_id);
                if let Some(constraint) = mapped.type_param.constraint
                    && !self
                        .bind_infer_defaults_inner(constraint, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if let Some(default) = mapped.type_param.default
                    && !self
                        .bind_infer_defaults_inner(default, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if !self.bind_infer_defaults_inner(
                    mapped.constraint,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) {
                    return false;
                }
                if let Some(name_type) = mapped.name_type
                    && !self
                        .bind_infer_defaults_inner(name_type, inferred, bindings, checker, visited)
                {
                    return false;
                }
                self.bind_infer_defaults_inner(
                    mapped.template,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeData::IndexAccess(obj, idx) => {
                self.bind_infer_defaults_inner(obj, inferred, bindings, checker, visited)
                    && self.bind_infer_defaults_inner(idx, inferred, bindings, checker, visited)
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.bind_infer_defaults_inner(inner, inferred, bindings, checker, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner().template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span
                        && !self
                            .bind_infer_defaults_inner(*inner, inferred, bindings, checker, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.bind_infer_defaults_inner(type_arg, inferred, bindings, checker, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.bind_infer_defaults_inner(member_type, inferred, bindings, checker, visited)
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => true,
        }
    }

    /// Match tuple elements against a pattern, extracting infer bindings.
    pub(crate) fn match_tuple_elements(
        &self,
        source_elems: &[TupleElement],
        pattern_elems: &[TupleElement],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let source_len = source_elems.len();
        let pattern_len = pattern_elems.len();

        let mut rest_index = None;
        for (idx, elem) in pattern_elems.iter().enumerate() {
            if elem.rest {
                if rest_index.is_some() {
                    return false;
                }
                rest_index = Some(idx);
            }
        }

        if let Some(rest_index) = rest_index {
            if rest_index + 1 != pattern_len {
                return false;
            }
            if source_len < rest_index {
                return false;
            }

            for i in 0..rest_index {
                let source_elem = &source_elems[i];
                let pattern_elem = &pattern_elems[i];
                if source_elem.rest || pattern_elem.rest {
                    return false;
                }
                let source_type = if source_elem.optional {
                    self.interner()
                        .union2(source_elem.type_id, TypeId::UNDEFINED)
                } else {
                    source_elem.type_id
                };
                if !self.match_infer_pattern(
                    source_type,
                    pattern_elem.type_id,
                    bindings,
                    visited,
                    checker,
                ) {
                    return false;
                }
            }

            let mut rest_elems = Vec::new();
            for source_elem in &source_elems[rest_index..] {
                if source_elem.rest {
                    return false;
                }
                rest_elems.push(TupleElement {
                    type_id: source_elem.type_id,
                    name: source_elem.name,
                    optional: source_elem.optional,
                    rest: false,
                });
            }

            let rest_tuple = self.interner().tuple(rest_elems);
            return self.match_infer_pattern(
                rest_tuple,
                pattern_elems[rest_index].type_id,
                bindings,
                visited,
                checker,
            );
        }

        if source_len > pattern_len {
            return false;
        }

        let shared = std::cmp::min(source_len, pattern_len);
        for i in 0..shared {
            let source_elem = &source_elems[i];
            let pattern_elem = &pattern_elems[i];
            if source_elem.rest || pattern_elem.rest {
                return false;
            }
            let source_type = if source_elem.optional {
                self.interner()
                    .union2(source_elem.type_id, TypeId::UNDEFINED)
            } else {
                source_elem.type_id
            };
            if !self.match_infer_pattern(
                source_type,
                pattern_elem.type_id,
                bindings,
                visited,
                checker,
            ) {
                return false;
            }
        }

        if source_len < pattern_len {
            for pattern_elem in &pattern_elems[source_len..] {
                if pattern_elem.rest {
                    return false;
                }
                if !pattern_elem.optional {
                    return false;
                }
                if self.type_contains_infer(pattern_elem.type_id)
                    && !self.match_infer_pattern(
                        TypeId::UNDEFINED,
                        pattern_elem.type_id,
                        bindings,
                        visited,
                        checker,
                    )
                {
                    return false;
                }
            }
        }

        true
    }

    /// Match function signature parameters against a pattern.
    pub(crate) fn match_signature_params(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if source_params.len() != pattern_params.len() {
            return false;
        }
        for (source_param, pattern_param) in source_params.iter().zip(pattern_params.iter()) {
            if source_param.optional != pattern_param.optional
                || source_param.rest != pattern_param.rest
            {
                return false;
            }
            // For optional params, add undefined to the source type for pattern matching.
            // This allows inferring T | undefined from optional params.
            let source_param_type = if source_param.optional {
                self.interner()
                    .union2(source_param.type_id, TypeId::UNDEFINED)
            } else {
                source_param.type_id
            };
            if !self.match_infer_pattern(
                source_param_type,
                pattern_param.type_id,
                bindings,
                visited,
                checker,
            ) {
                return false;
            }
        }
        true
    }

    /// Main pattern matching function for infer types.
    ///
    /// Matches a source type against a pattern containing `infer` types,
    /// extracting the bound values into the bindings map.
    ///
    /// # Arguments
    /// * `source` - The concrete type to match against
    /// * `pattern` - The pattern type containing `infer` placeholders
    /// * `bindings` - Map to store extracted type bindings
    /// * `visited` - Set of already-visited type pairs (for cycle detection)
    /// * `checker` - Subtype checker for constraint validation
    ///
    /// # Returns
    /// `true` if the match succeeded and all bindings were extracted
    pub(crate) fn match_infer_pattern(
        &self,
        source: TypeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if !visited.insert((source, pattern)) {
            return true;
        }

        if source == TypeId::NEVER {
            return self.bind_infer_defaults(pattern, TypeId::NEVER, bindings, checker);
        }

        if source == pattern {
            return true;
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(source) {
            let members = self.interner().type_list(members);
            let base = bindings.clone();
            let mut merged = base.clone();

            for &member in members.iter() {
                let mut local = base.clone();
                if !self.match_infer_pattern(member, pattern, &mut local, visited, checker) {
                    return false;
                }

                for (name, ty) in local {
                    if base.contains_key(&name) {
                        continue;
                    }

                    if let Some(existing) = merged.get_mut(&name) {
                        if *existing != ty {
                            *existing = self.interner().union2(*existing, ty);
                        }
                    } else {
                        merged.insert(name, ty);
                    }
                }
            }

            *bindings = merged;
            return true;
        }

        let Some(pattern_key) = self.interner().lookup(pattern) else {
            return false;
        };

        match pattern_key {
            TypeData::Infer(info) => self.bind_infer(&info, source, bindings, checker),
            TypeData::Function(pattern_fn_id) => self.match_infer_function_pattern(
                source,
                pattern_fn_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Callable(pattern_shape_id) => self.match_infer_callable_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Array(pattern_elem) => match self.interner().lookup(source) {
                Some(TypeData::Array(source_elem)) => {
                    self.match_infer_pattern(source_elem, pattern_elem, bindings, visited, checker)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeData::Array(source_elem)) = self.interner().lookup(member)
                        else {
                            return false;
                        };
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            source_elem,
                            pattern_elem,
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
            },
            TypeData::Tuple(pattern_elems) => match self.interner().lookup(source) {
                Some(TypeData::Tuple(source_elems)) => {
                    let source_elems = self.interner().tuple_list(source_elems);
                    let pattern_elems = self.interner().tuple_list(pattern_elems);
                    self.match_tuple_elements(
                        &source_elems,
                        &pattern_elems,
                        bindings,
                        visited,
                        checker,
                    )
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeData::Tuple(source_elems)) = self.interner().lookup(member)
                        else {
                            return false;
                        };
                        let source_elems = self.interner().tuple_list(source_elems);
                        let pattern_elems = self.interner().tuple_list(pattern_elems);
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_tuple_elements(
                            &source_elems,
                            &pattern_elems,
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
            },
            TypeData::ReadonlyType(pattern_inner) => {
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::ReadonlyType(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::NoInfer(pattern_inner) => {
                // NoInfer<T> matches if source matches T (strip wrapper)
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::NoInfer(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::Object(pattern_shape_id) => self.match_infer_object_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::ObjectWithIndex(pattern_shape_id) => self
                .match_infer_object_with_index_pattern(
                    source,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ),
            TypeData::Application(pattern_app_id) => {
                // First try declaration matching: Application vs Application
                let declaration_matched = match self.interner().lookup(source) {
                    Some(TypeData::Application(source_app_id)) => {
                        let source_app = self.interner().type_application(source_app_id);
                        let pattern_app = self.interner().type_application(pattern_app_id);
                        if source_app.args.len() != pattern_app.args.len() {
                            return false;
                        }
                        if !checker.is_subtype_of(source_app.base, pattern_app.base)
                            || !checker.is_subtype_of(pattern_app.base, source_app.base)
                        {
                            return false;
                        }
                        for (source_arg, pattern_arg) in
                            source_app.args.iter().zip(pattern_app.args.iter())
                        {
                            if !self.match_infer_pattern(
                                *source_arg,
                                *pattern_arg,
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
                };

                // If declaration matching succeeded, we're done
                if declaration_matched {
                    return true;
                }

                // Fallback: Structural expansion
                // Expand the pattern Application to its structural form and recurse
                // This handles cases like: Reducer<infer S> matching a structural function type
                let evaluator = ApplicationEvaluator::new(self.interner(), self.resolver());
                let expanded_pattern = evaluator.evaluate_or_original(pattern);

                // Only recurse if expansion actually changed the type
                if expanded_pattern != pattern {
                    return self.match_infer_pattern(
                        source,
                        expanded_pattern,
                        bindings,
                        visited,
                        checker,
                    );
                }

                false
            }
            TypeData::TemplateLiteral(pattern_spans_id) => {
                let pattern_spans = self.interner().template_list(pattern_spans_id);
                match self.interner().lookup(source) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        let source_text = self.interner().resolve_atom_ref(atom);
                        self.match_template_literal_string(
                            source_text.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeData::TemplateLiteral(source_spans_id)) => {
                        let source_spans = self.interner().template_list(source_spans_id);
                        self.match_template_literal_spans(
                            source,
                            source_spans.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeData::Intrinsic(IntrinsicKind::String)) => self
                        .match_template_literal_string_type(
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        ),
                    _ => false,
                }
            }
            // Handle union pattern containing infer types
            // Pattern: infer S | T | U where S is infer and T, U are not
            // Source: A | T | U or a single type A
            // Algorithm: Match source members against non-infer pattern members,
            // then bind the infer to the remaining source members
            TypeData::Union(pattern_members) => {
                self.match_infer_union_pattern(source, pattern_members, pattern, bindings, checker)
            }
            _ => checker.is_subtype_of(source, pattern),
        }
    }
}
