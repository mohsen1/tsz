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

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::application::ApplicationEvaluator;
use crate::solver::subtype::{SubtypeChecker, TypeResolver};
use crate::solver::types::*;
use crate::solver::utils;
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::evaluate::TypeEvaluator;

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
            TypeKey::Infer(_) => true,
            TypeKey::Array(elem) => self.type_contains_infer_inner(elem, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                elements
                    .iter()
                    .any(|element| self.type_contains_infer_inner(element.type_id, visited))
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner().type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_infer_inner(member, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeKey::ObjectWithIndex(shape_id) => {
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
            TypeKey::Function(shape_id) => {
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
            TypeKey::Callable(shape_id) => {
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
            TypeKey::TypeParameter(info) => {
                info.constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || info
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
            }
            TypeKey::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                self.type_contains_infer_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_infer_inner(arg, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner().conditional_type(cond_id);
                self.type_contains_infer_inner(cond.check_type, visited)
                    || self.type_contains_infer_inner(cond.extends_type, visited)
                    || self.type_contains_infer_inner(cond.true_type, visited)
                    || self.type_contains_infer_inner(cond.false_type, visited)
            }
            TypeKey::Mapped(mapped_id) => {
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
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_infer_inner(obj, visited)
                    || self.type_contains_infer_inner(idx, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.type_contains_infer_inner(inner, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner().template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_infer_inner(*inner, visited),
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.type_contains_infer_inner(type_arg, visited)
            }
            TypeKey::Enum(_def_id, member_type) => {
                self.type_contains_infer_inner(member_type, visited)
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::ModuleNamespace(_)
            | TypeKey::Error => false,
        }
    }

    /// Filter an inferred type by its constraint.
    ///
    /// Returns Some(filtered_type) if any part of the inferred type satisfies the constraint,
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

        if let Some(TypeKey::Union(members)) = self.interner().lookup(inferred) {
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

        if checker.is_subtype_of(inferred, constraint) {
            Some(inferred)
        } else {
            None
        }
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

        if let Some(TypeKey::Union(members)) = self.interner().lookup(inferred) {
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
            TypeKey::Infer(info) => self.bind_infer(&info, inferred, bindings, checker),
            TypeKey::Array(elem) => {
                self.bind_infer_defaults_inner(elem, inferred, bindings, checker, visited)
            }
            TypeKey::Tuple(elements) => {
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
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if !self.bind_infer_defaults_inner(member, inferred, bindings, checker, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in shape.properties.iter() {
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
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in shape.properties.iter() {
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
            TypeKey::Function(shape_id) => {
                let shape = self.interner().function_shape(shape_id);
                for param in shape.params.iter() {
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
            TypeKey::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                for sig in shape.call_signatures.iter() {
                    for param in sig.params.iter() {
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
                for sig in shape.construct_signatures.iter() {
                    for param in sig.params.iter() {
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
                for prop in shape.properties.iter() {
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
            TypeKey::TypeParameter(info) => {
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
            TypeKey::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                if !self.bind_infer_defaults_inner(app.base, inferred, bindings, checker, visited) {
                    return false;
                }
                for &arg in app.args.iter() {
                    if !self.bind_infer_defaults_inner(arg, inferred, bindings, checker, visited) {
                        return false;
                    }
                }
                true
            }
            TypeKey::Conditional(cond_id) => {
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
            TypeKey::Mapped(mapped_id) => {
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
            TypeKey::IndexAccess(obj, idx) => {
                self.bind_infer_defaults_inner(obj, inferred, bindings, checker, visited)
                    && self.bind_infer_defaults_inner(idx, inferred, bindings, checker, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.bind_infer_defaults_inner(inner, inferred, bindings, checker, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
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
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.bind_infer_defaults_inner(type_arg, inferred, bindings, checker, visited)
            }
            TypeKey::Enum(_def_id, member_type) => {
                self.bind_infer_defaults_inner(member_type, inferred, bindings, checker, visited)
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::ModuleNamespace(_)
            | TypeKey::Error => true,
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
        eprintln!(
            "DEBUG match_infer_pattern: source={} pattern={}",
            source.0, pattern.0
        );

        if !visited.insert((source, pattern)) {
            return true;
        }

        if source == TypeId::NEVER {
            return self.bind_infer_defaults(pattern, TypeId::NEVER, bindings, checker);
        }

        if source == pattern {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner().lookup(source) {
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
            TypeKey::Infer(info) => self.bind_infer(&info, source, bindings, checker),
            TypeKey::Function(pattern_fn_id) => self.match_infer_function_pattern(
                source,
                pattern_fn_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeKey::Callable(pattern_shape_id) => self.match_infer_callable_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeKey::Array(pattern_elem) => match self.interner().lookup(source) {
                Some(TypeKey::Array(source_elem)) => {
                    self.match_infer_pattern(source_elem, pattern_elem, bindings, visited, checker)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeKey::Array(source_elem)) = self.interner().lookup(member)
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
            TypeKey::Tuple(pattern_elems) => match self.interner().lookup(source) {
                Some(TypeKey::Tuple(source_elems)) => {
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
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeKey::Tuple(source_elems)) = self.interner().lookup(member)
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
            TypeKey::ReadonlyType(pattern_inner) => {
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeKey::ReadonlyType(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeKey::Object(pattern_shape_id) => self.match_infer_object_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeKey::ObjectWithIndex(pattern_shape_id) => self
                .match_infer_object_with_index_pattern(
                    source,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ),
            TypeKey::Application(pattern_app_id) => {
                // First try declaration matching: Application vs Application
                let declaration_matched = match self.interner().lookup(source) {
                    Some(TypeKey::Application(source_app_id)) => {
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

                eprintln!("DEBUG match_infer_pattern: Application expansion");
                eprintln!("  pattern={} expanded={}", pattern.0, expanded_pattern.0);

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
            TypeKey::TemplateLiteral(pattern_spans_id) => {
                let pattern_spans = self.interner().template_list(pattern_spans_id);
                match self.interner().lookup(source) {
                    Some(TypeKey::Literal(LiteralValue::String(atom))) => {
                        let source_text = self.interner().resolve_atom_ref(atom);
                        self.match_template_literal_string(
                            source_text.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeKey::TemplateLiteral(source_spans_id)) => {
                        let source_spans = self.interner().template_list(source_spans_id);
                        self.match_template_literal_spans(
                            source,
                            source_spans.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeKey::Intrinsic(IntrinsicKind::String)) => self
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
            TypeKey::Union(pattern_members) => {
                self.match_infer_union_pattern(source, pattern_members, pattern, bindings, checker)
            }
            _ => checker.is_subtype_of(source, pattern),
        }
    }

    /// Helper for matching function type patterns.
    fn match_infer_function_pattern(
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

        if pattern_fn.this_type.is_none() && has_param_infer && has_return_infer {
            // Check if pattern has a single rest parameter (e.g., (...args: any[]) => infer R)
            // This should match any function signature and only extract the return type
            let has_single_rest_param = pattern_fn.params.len() == 1 && pattern_fn.params[0].rest;

            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if has_single_rest_param {
                    // For a pattern like (...args: any[]) => infer R, we only care about
                    // matching the return type. The parameters are ignored.
                    // However, if the pattern parameter type contains infer, we still need to match it.
                    if self.type_contains_infer(pattern_fn.params[0].type_id) {
                        let pattern_param = &pattern_fn.params[0];
                        for source_param in source_params {
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
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                        }
                    }
                    // If the pattern param doesn't contain infer, skip parameter matching entirely
                } else {
                    if !self.match_signature_params(
                        source_params,
                        &pattern_fn.params,
                        bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
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
                Some(TypeKey::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_params_and_return(
                        source,
                        &source_fn.params,
                        source_fn.return_type,
                        bindings,
                    )
                }
                Some(TypeKey::Callable(source_shape_id)) => {
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
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Function(source_fn_id)) => {
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
                            Some(TypeKey::Callable(source_shape_id)) => {
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
                _ => false,
            };
        }

        if pattern_fn.this_type.is_none() && has_param_infer && !has_return_infer {
            // Handle constructor function patterns differently
            if pattern_fn.is_constructor {
                return self.match_infer_constructor_pattern(
                    source,
                    &pattern_fn,
                    bindings,
                    checker,
                );
            }

            // Regular function parameter inference
            let mut match_function_params = |_source_type: TypeId,
                                             source_fn_id: FunctionShapeId,
                                             bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let source_fn = self.interner().function_shape(source_fn_id);
                if source_fn.params.len() != pattern_fn.params.len() {
                    return false;
                }
                let mut local_visited = FxHashSet::default();
                for (source_param, pattern_param) in
                    source_fn.params.iter().zip(pattern_fn.params.iter())
                {
                    if source_param.optional != pattern_param.optional
                        || source_param.rest != pattern_param.rest
                    {
                        return false;
                    }
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
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                // For param-only inference, parameter matching is sufficient.
                // Skipping the final subtype check avoids issues with optional
                // param widening (undefined added twice).
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeKey::Function(source_fn_id)) => {
                    match_function_params(source, source_fn_id, bindings)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeKey::Function(source_fn_id)) = self.interner().lookup(member)
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
                Some(TypeKey::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_return(source, source_fn.return_type, bindings)
                }
                Some(TypeKey::Callable(source_shape_id)) => {
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
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if !match_return(
                                    member,
                                    source_fn.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeKey::Callable(source_shape_id)) => {
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
            Some(TypeKey::Function(source_fn_id)) => {
                match_function_this(source, source_fn_id, bindings)
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let Some(TypeKey::Function(source_fn_id)) = self.interner().lookup(member)
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
    fn match_infer_constructor_pattern(
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
                Some(TypeKey::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.construct_signatures.is_empty() {
                        return false;
                    }
                    let source_sig = &source_shape.construct_signatures[0];
                    match_construct_params_tuple(&source_sig.params, bindings)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Callable(source_shape_id)) => {
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
            Some(TypeKey::Callable(source_shape_id)) => {
                let source_shape = self.interner().callable_shape(source_shape_id);
                if source_shape.construct_signatures.is_empty() {
                    return false;
                }
                let source_sig = &source_shape.construct_signatures[0];
                match_construct_params(&source_sig.params, bindings)
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    match self.interner().lookup(member) {
                        Some(TypeKey::Callable(source_shape_id)) => {
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
    fn match_infer_callable_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        _visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);
        if pattern_shape.call_signatures.len() != 1
            || !pattern_shape.construct_signatures.is_empty()
            || !pattern_shape.properties.is_empty()
        {
            return checker.is_subtype_of(source, pattern);
        }
        let pattern_sig = &pattern_shape.call_signatures[0];
        let has_param_infer = pattern_sig
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_sig.return_type);
        if pattern_sig.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if !self.match_signature_params(
                    source_params,
                    &pattern_sig.params,
                    bindings,
                    &mut local_visited,
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
                Some(TypeKey::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.len() != 1
                        || !source_shape.construct_signatures.is_empty()
                        || !source_shape.properties.is_empty()
                    {
                        return false;
                    }
                    let source_sig = &source_shape.call_signatures[0];
                    match_params_and_return(
                        source,
                        &source_sig.params,
                        source_sig.return_type,
                        bindings,
                    )
                }
                Some(TypeKey::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_params_and_return(
                        source,
                        &source_fn.params,
                        source_fn.return_type,
                        bindings,
                    )
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.len() != 1
                                    || !source_shape.construct_signatures.is_empty()
                                    || !source_shape.properties.is_empty()
                                {
                                    return false;
                                }
                                let source_sig = &source_shape.call_signatures[0];
                                if !match_params_and_return(
                                    member,
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeKey::Function(source_fn_id)) => {
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
                    let mut local_visited = FxHashSet::default();
                    // Match params and infer types. Skip subtype check since pattern matching
                    // success implies compatibility. The subtype check can fail for optional
                    // params due to contravariance issues with undefined.
                    self.match_signature_params(
                        source_params,
                        &pattern_sig.params,
                        bindings,
                        &mut local_visited,
                        checker,
                    )
                };

            return match self.interner().lookup(source) {
                Some(TypeKey::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.len() != 1
                        || !source_shape.construct_signatures.is_empty()
                        || !source_shape.properties.is_empty()
                    {
                        return false;
                    }
                    let source_sig = &source_shape.call_signatures[0];
                    match_params(&source_sig.params, bindings)
                }
                Some(TypeKey::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_params(&source_fn.params, bindings)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.len() != 1
                                    || !source_shape.construct_signatures.is_empty()
                                    || !source_shape.properties.is_empty()
                                {
                                    return false;
                                }
                                let source_sig = &source_shape.call_signatures[0];
                                if !match_params(&source_sig.params, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeKey::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
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
                Some(TypeKey::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.len() != 1
                        || !source_shape.construct_signatures.is_empty()
                        || !source_shape.properties.is_empty()
                    {
                        return false;
                    }
                    let source_sig = &source_shape.call_signatures[0];
                    match_return(source, source_sig.return_type, bindings)
                }
                Some(TypeKey::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    match_return(source, source_fn.return_type, bindings)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeKey::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.len() != 1
                                    || !source_shape.construct_signatures.is_empty()
                                    || !source_shape.properties.is_empty()
                                {
                                    return false;
                                }
                                let source_sig = &source_shape.call_signatures[0];
                                if !match_return(
                                    member,
                                    source_sig.return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeKey::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
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
    fn match_infer_object_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        match self.interner().lookup(source) {
            Some(TypeKey::Object(source_shape_id))
            | Some(TypeKey::ObjectWithIndex(source_shape_id)) => {
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
            Some(TypeKey::Intersection(members)) => {
                let members = self.interner().type_list(members);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let mut merged_type = None;
                    for &member in members.iter() {
                        let shape_id = match self.interner().lookup(member) {
                            Some(TypeKey::Object(shape_id))
                            | Some(TypeKey::ObjectWithIndex(shape_id)) => shape_id,
                            _ => return false,
                        };
                        let shape = self.interner().object_shape(shape_id);
                        if let Some(source_prop) = shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == pattern_prop.name)
                        {
                            let source_type = self.optional_property_type(source_prop);
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
            Some(TypeKey::Union(members)) => {
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

    /// Helper for matching object with index type patterns.
    fn match_infer_object_with_index_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        match self.interner().lookup(source) {
            Some(TypeKey::Object(source_shape_id))
            | Some(TypeKey::ObjectWithIndex(source_shape_id)) => {
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
            Some(TypeKey::Union(members)) => {
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
    fn match_infer_union_pattern(
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
            if let Some(TypeKey::Infer(info)) = self.interner().lookup(pattern_member) {
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
            Some(TypeKey::Union(source_members)) => {
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
                    let end = if let Some(next_text) = next_text {
                        let next_value = self.interner().resolve_atom_ref(next_text);
                        match source[pos..].find(next_value.as_ref()) {
                            Some(offset) => pos + offset,
                            None => return false,
                        }
                    } else {
                        source.len()
                    };

                    let captured = &source[pos..end];
                    pos = end;
                    let captured_type = self.interner().literal_string(captured);

                    if let Some(TypeKey::Infer(info)) = self.interner().lookup(type_id) {
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
            if let Some(TypeKey::Infer(info)) = self.interner().lookup(type_id) {
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
                    if let Some(TypeKey::Infer(info)) = self.interner().lookup(*type_id) {
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
                if let Some(TypeKey::Infer(info)) = self.interner().lookup(*type_id) {
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

/// Helper for substituting infer bindings into types.
///
/// This struct performs a deep traversal of a type, replacing all `infer X`
/// references with their bound values from the bindings map.
pub(crate) struct InferSubstitutor<'a> {
    interner: &'a dyn TypeDatabase,
    bindings: &'a FxHashMap<Atom, TypeId>,
    visiting: FxHashMap<TypeId, TypeId>,
}

impl<'a> InferSubstitutor<'a> {
    /// Create a new substitutor with the given interner and bindings.
    pub fn new(interner: &'a dyn TypeDatabase, bindings: &'a FxHashMap<Atom, TypeId>) -> Self {
        InferSubstitutor {
            interner,
            bindings,
            visiting: FxHashMap::default(),
        }
    }

    /// Substitute infer types in the given type, returning the result.
    pub fn substitute(&mut self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.visiting.get(&type_id) {
            return cached;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return type_id;
        };

        self.visiting.insert(type_id, type_id);

        let result = match key {
            TypeKey::Infer(info) => self.bindings.get(&info.name).copied().unwrap_or(type_id),
            TypeKey::Array(elem) => {
                let substituted = self.substitute(elem);
                if substituted == elem {
                    type_id
                } else {
                    self.interner.array(substituted)
                }
            }
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                let mut changed = false;
                let mut new_elements = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let substituted = self.substitute(element.type_id);
                    if substituted != element.type_id {
                        changed = true;
                    }
                    new_elements.push(TupleElement {
                        type_id: substituted,
                        name: element.name,
                        optional: element.optional,
                        rest: element.rest,
                    });
                }
                if changed {
                    self.interner.tuple(new_elements)
                } else {
                    type_id
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.union(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.intersection(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in shape.properties.iter() {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }
                if changed {
                    self.interner.object(properties)
                } else {
                    type_id
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in shape.properties.iter() {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                        visibility: prop.visibility,
                        parent_id: prop.parent_id,
                    });
                }
                let string_index = shape.string_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                let number_index = shape.number_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                if changed {
                    self.interner.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties,
                        string_index,
                        number_index,
                        symbol: None,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                let check_type = self.substitute(cond.check_type);
                let extends_type = self.substitute(cond.extends_type);
                let true_type = self.substitute(cond.true_type);
                let false_type = self.substitute(cond.false_type);
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    type_id
                } else {
                    self.interner.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                }
            }
            TypeKey::IndexAccess(obj, idx) => {
                let new_obj = self.substitute(obj);
                let new_idx = self.substitute(idx);
                if new_obj == obj && new_idx == idx {
                    type_id
                } else {
                    self.interner.intern(TypeKey::IndexAccess(new_obj, new_idx))
                }
            }
            TypeKey::KeyOf(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.intern(TypeKey::KeyOf(new_inner))
                }
            }
            TypeKey::ReadonlyType(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.intern(TypeKey::ReadonlyType(new_inner))
                }
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                let mut changed = false;
                let mut new_spans = Vec::with_capacity(spans.len());
                for span in spans.iter() {
                    let new_span = match span {
                        TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                        TemplateSpan::Type(inner) => {
                            let substituted = self.substitute(*inner);
                            if substituted != *inner {
                                changed = true;
                            }
                            TemplateSpan::Type(substituted)
                        }
                    };
                    new_spans.push(new_span);
                }
                if changed {
                    self.interner.template_literal(new_spans)
                } else {
                    type_id
                }
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let base = self.substitute(app.base);
                let mut changed = base != app.base;
                let mut new_args = Vec::with_capacity(app.args.len());
                for &arg in &app.args {
                    let substituted = self.substitute(arg);
                    if substituted != arg {
                        changed = true;
                    }
                    new_args.push(substituted);
                }
                if changed {
                    self.interner.application(base, new_args)
                } else {
                    type_id
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let mut changed = false;
                let mut new_params = Vec::with_capacity(shape.params.len());
                for param in shape.params.iter() {
                    let param_type = self.substitute(param.type_id);
                    if param_type != param.type_id {
                        changed = true;
                    }
                    new_params.push(ParamInfo {
                        name: param.name,
                        type_id: param_type,
                        optional: param.optional,
                        rest: param.rest,
                    });
                }
                let return_type = self.substitute(shape.return_type);
                if return_type != shape.return_type {
                    changed = true;
                }
                let this_type = shape.this_type.map(|t| {
                    let substituted = self.substitute(t);
                    if substituted != t {
                        changed = true;
                    }
                    substituted
                });
                if changed {
                    self.interner.function(FunctionShape {
                        params: new_params,
                        this_type,
                        return_type,
                        type_params: shape.type_params.clone(),
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let mut changed = false;

                let call_signatures: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in sig.params.iter() {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                            is_method: sig.is_method,
                        }
                    })
                    .collect();

                let construct_signatures: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in sig.params.iter() {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                            is_method: sig.is_method,
                        }
                    })
                    .collect();

                let properties: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let prop_type = self.substitute(prop.type_id);
                        let write_type = self.substitute(prop.write_type);
                        if prop_type != prop.type_id || write_type != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: prop_type,
                            write_type,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                            visibility: prop.visibility,
                            parent_id: prop.parent_id,
                        }
                    })
                    .collect();

                let string_index = shape.string_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                let number_index = shape.number_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                if changed {
                    self.interner.callable(CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties,
                        string_index,
                        number_index,
                        symbol: None,
                    })
                } else {
                    type_id
                }
            }
            _ => type_id,
        };

        self.visiting.insert(type_id, result);
        result
    }
}
