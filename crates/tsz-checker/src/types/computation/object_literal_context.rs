//! Contextual property type resolution helpers for object literal expressions.
//!
//! Extracted from `object_literal.rs` to keep that file under the 2000 LOC limit.
//! Contains:
//! - `contextual_object_literal_property_type` — main contextual property type extraction
//! - `contextual_property_presence` — check if a property exists in a contextual type
//! - `fallback_contextual_callable_property_type` — callable property fallback
//! - `should_preserve_absent_contextual_property_type` — generic/mapped type preservation
//! - `union_with_non_nullish_non_object_member` — union member analysis
//! - `precise_callable_context_type` — callable context type extraction
//! - `function_initializer_context_type` — function initializer contextual type
//! - `check_destructuring_default_initializer` — destructuring default checking
//! - `destructuring_target_type_from_initializer` — destructuring target type inference
//! - `prefer_more_specific_contextual_property_type` — property type preference logic
//! - `sanitize_contextual_property_type` — contextual type sanitization

use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common::{self, ContextualTypeContext, TypeSubstitution};
use crate::query_boundaries::object_literal_context as object_context_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextualPropertyPresence {
    Present,
    Absent,
    Unknown,
}

impl<'a> CheckerState<'a> {
    fn is_this_type_marker_application(&self, type_id: TypeId) -> bool {
        let Some(app) = common::type_application(self.ctx.types, type_id) else {
            return false;
        };
        common::is_this_type(self.ctx.types, app.base)
    }

    pub(crate) fn strip_contextual_this_type_markers(&self, type_id: TypeId) -> TypeId {
        if self.is_this_type_marker_application(type_id) {
            return TypeId::UNKNOWN;
        }

        if let Some(members) = common::intersection_members(self.ctx.types, type_id) {
            let filtered: Vec<_> = members
                .iter()
                .copied()
                .filter(|&member| !self.is_this_type_marker_application(member))
                .collect();
            return match filtered.as_slice() {
                [] => TypeId::UNKNOWN,
                [single] => *single,
                _ if filtered.len() == members.len() => type_id,
                _ => object_context_query::contextual_intersection(self.ctx.types, filtered),
            };
        }
        if let Some(members) = common::union_members(self.ctx.types, type_id) {
            let remapped: Vec<_> = members
                .iter()
                .copied()
                .map(|member| self.strip_contextual_this_type_markers(member))
                .filter(|&member| member != TypeId::UNKNOWN)
                .collect();
            return match remapped.as_slice() {
                [] => TypeId::UNKNOWN,
                [single] => *single,
                _ if remapped.len() == members.len()
                    && remapped
                        .iter()
                        .zip(members.iter())
                        .all(|(left, right)| left == right) =>
                {
                    type_id
                }
                _ => object_context_query::contextual_union_preserve_members(
                    self.ctx.types,
                    remapped,
                ),
            };
        }

        type_id
    }

    pub(crate) fn named_contextual_property_presence(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> ContextualPropertyPresence {
        let contextual_type = self.strip_contextual_this_type_markers(contextual_type);
        self.contextual_property_presence(contextual_type, property_name, 6)
    }

    pub(crate) fn named_contextual_property_allows_callable_fallback(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> bool {
        let contextual_type = self.strip_contextual_this_type_markers(contextual_type);
        !matches!(
            self.contextual_property_presence(contextual_type, property_name, 6),
            ContextualPropertyPresence::Absent
        )
    }

    pub(crate) fn contextual_callable_property_fallback_type(
        &mut self,
        contextual_type: TypeId,
        property_context_type: Option<TypeId>,
    ) -> Option<TypeId> {
        let contextual_type = self.strip_contextual_this_type_markers(contextual_type);
        let wildcard_context = self.contextual_object_literal_property_type(contextual_type, "*");
        let callable_fallback = self.fallback_contextual_callable_property_type(contextual_type, 6);

        match wildcard_context {
            Some(TypeId::ANY | TypeId::UNKNOWN) | None => callable_fallback
                .or(wildcard_context)
                .or(property_context_type),
            Some(_) => wildcard_context
                .or(callable_fallback)
                .or(property_context_type),
        }
    }

    pub(crate) fn contextual_callable_string_index_signature_type(
        &mut self,
        type_id: TypeId,
        depth: usize,
    ) -> Option<TypeId> {
        if depth == 0 {
            return None;
        }

        let type_id = self.strip_contextual_this_type_markers(type_id);
        let mut candidates = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut push_candidate = |this: &mut Self, candidate: TypeId| {
            if let Some(callable) = this.precise_callable_context_type(candidate)
                && seen.insert(callable)
            {
                candidates.push(callable);
            }
        };

        if let Some(index) = self.ctx.types.get_index_signatures(type_id).string_index {
            push_candidate(self, index.value_type);
        }

        if let Some(constraint) = common::type_parameter_constraint(self.ctx.types, type_id)
            && let Some(candidate) =
                self.contextual_callable_string_index_signature_type(constraint, depth - 1)
        {
            push_candidate(self, candidate);
        }

        if let Some(members) = common::union_members(self.ctx.types, type_id)
            .or_else(|| common::intersection_members(self.ctx.types, type_id))
        {
            for member in members {
                if let Some(candidate) =
                    self.contextual_callable_string_index_signature_type(member, depth - 1)
                {
                    push_candidate(self, candidate);
                }
            }
        }

        let resolved_type = self.resolve_type_for_property_access(type_id);
        if resolved_type != type_id
            && let Some(candidate) =
                self.contextual_callable_string_index_signature_type(resolved_type, depth - 1)
        {
            push_candidate(self, candidate);
        }

        let evaluated_type = self.evaluate_type_with_env(type_id);
        let evaluated_type = self.resolve_type_for_property_access(evaluated_type);
        let evaluated_type = self.resolve_lazy_type(evaluated_type);
        let evaluated_type = self.evaluate_application_type(evaluated_type);
        if evaluated_type != type_id
            && evaluated_type != resolved_type
            && let Some(candidate) =
                self.contextual_callable_string_index_signature_type(evaluated_type, depth - 1)
        {
            push_candidate(self, candidate);
        }

        match candidates.as_slice() {
            [] => None,
            [single] => Some(*single),
            _ => Some(object_context_query::contextual_union_preserve_members(
                self.ctx.types,
                candidates,
            )),
        }
    }

    pub(crate) fn fallback_contextual_callable_property_type(
        &mut self,
        type_id: TypeId,
        depth: usize,
    ) -> Option<TypeId> {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;

        if depth == 0 {
            return None;
        }

        let mut candidates = Vec::new();

        let resolved_type = self.resolve_type_for_property_access(type_id);
        if resolved_type != type_id
            && let Some(candidate) =
                self.fallback_contextual_callable_property_type(resolved_type, depth - 1)
        {
            candidates.push(candidate);
        }

        let evaluated_type = self.evaluate_type_with_env(type_id);
        let evaluated_type = self.resolve_type_for_property_access(evaluated_type);
        let evaluated_type = self.resolve_lazy_type(evaluated_type);
        let evaluated_type = self.evaluate_application_type(evaluated_type);
        if evaluated_type != type_id
            && evaluated_type != resolved_type
            && let Some(candidate) =
                self.fallback_contextual_callable_property_type(evaluated_type, depth - 1)
        {
            candidates.push(candidate);
        }

        match crate::query_boundaries::assignability::classify_for_excess_properties(
            self.ctx.types,
            type_id,
        ) {
            ExcessPropertiesKind::Object(_) | ExcessPropertiesKind::ObjectWithIndex(_) => {
                // Delegate to solver query: collect all callable property types
                // (named properties + index signatures) from the object shape.
                candidates.extend(common::collect_callable_property_types(
                    self.ctx.types,
                    type_id,
                ));
            }
            ExcessPropertiesKind::Union(members) | ExcessPropertiesKind::Intersection(members) => {
                for member in members {
                    if let Some(candidate) =
                        self.fallback_contextual_callable_property_type(member, depth - 1)
                    {
                        candidates.push(candidate);
                    }
                }
            }
            ExcessPropertiesKind::NotObject => {}
        }

        if candidates.is_empty() {
            None
        } else if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            Some(object_context_query::contextual_union_preserve_members(
                self.ctx.types,
                candidates,
            ))
        }
    }

    fn should_preserve_absent_contextual_property_type(
        &mut self,
        type_id: TypeId,
        depth: usize,
    ) -> bool {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;

        if depth == 0 {
            return false;
        }

        if common::contains_type_parameters(self.ctx.types, type_id)
            || common::is_mapped_type(self.ctx.types, type_id)
            || common::type_application(self.ctx.types, type_id).is_some()
        {
            return true;
        }

        let resolved_type = self.resolve_type_for_property_access(type_id);
        if resolved_type != type_id
            && self.should_preserve_absent_contextual_property_type(resolved_type, depth - 1)
        {
            return true;
        }

        let evaluated_type = self.evaluate_type_with_env(type_id);
        let evaluated_type = self.resolve_type_for_property_access(evaluated_type);
        let evaluated_type = self.resolve_lazy_type(evaluated_type);
        let evaluated_type = self.evaluate_application_type(evaluated_type);
        if evaluated_type != type_id
            && evaluated_type != resolved_type
            && self.should_preserve_absent_contextual_property_type(evaluated_type, depth - 1)
        {
            return true;
        }

        match crate::query_boundaries::assignability::classify_for_excess_properties(
            self.ctx.types,
            type_id,
        ) {
            ExcessPropertiesKind::Union(members) | ExcessPropertiesKind::Intersection(members) => {
                members.into_iter().any(|member| {
                    self.should_preserve_absent_contextual_property_type(member, depth - 1)
                })
            }
            _ => false,
        }
    }

    fn union_with_non_nullish_non_object_member(&mut self, type_id: TypeId, depth: usize) -> bool {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;

        if depth == 0 {
            return false;
        }

        let evaluated_type = self.evaluate_type_with_env(type_id);
        let evaluated_type = self.resolve_lazy_type(evaluated_type);
        let evaluated_type = self.evaluate_application_type(evaluated_type);

        if self.ctx.types.is_nullish_type(evaluated_type) {
            return false;
        }

        match crate::query_boundaries::assignability::classify_for_excess_properties(
            self.ctx.types,
            evaluated_type,
        ) {
            ExcessPropertiesKind::Object(_) | ExcessPropertiesKind::ObjectWithIndex(_) => false,
            ExcessPropertiesKind::Union(members) => members
                .iter()
                .copied()
                .any(|member| self.union_with_non_nullish_non_object_member(member, depth - 1)),
            ExcessPropertiesKind::Intersection(members) => members
                .iter()
                .copied()
                .any(|member| self.union_with_non_nullish_non_object_member(member, depth - 1)),
            ExcessPropertiesKind::NotObject => {
                if common::is_primitive_type(self.ctx.types, evaluated_type) {
                    return true;
                }

                let resolved_type = self.resolve_type_for_property_access(evaluated_type);
                if resolved_type != evaluated_type {
                    return self.union_with_non_nullish_non_object_member(resolved_type, depth - 1);
                }

                false
            }
        }
    }

    /// Returns true if any non-nullish, non-object union member of `type_id` has the
    /// given property accessible via its wrapper interface (e.g. `String.prototype.normalize`
    /// for the `string` primitive).
    ///
    /// Used to detect the "contextual overload list from union with primitive" case: when
    /// `string | SomeObject` is the contextual type and the `string` wrapper (`String`)
    /// also has the property in question, the two signatures conflict and tsc does not
    /// provide a contextual type for callback parameters (-> TS7006). This is distinct from
    /// properties that only exist on the object member (e.g. `validate` on `string | FullRule`
    /// where `String` has no `validate`), which should still be contextually typed.
    pub(crate) fn primitive_union_member_has_property(
        &mut self,
        type_id: TypeId,
        property_name: &str,
    ) -> bool {
        use crate::query_boundaries::assignability::{
            ExcessPropertiesKind, classify_for_excess_properties,
        };
        use common::PropertyAccessResult;

        let resolved = self.resolve_type_for_property_access(type_id);
        let evaluated = self.evaluate_type_with_env(type_id);
        let evaluated = self.resolve_type_for_property_access(evaluated);
        let evaluated = self.resolve_lazy_type(evaluated);
        let evaluated = self.evaluate_application_type(evaluated);

        let members = common::union_members(self.ctx.types, type_id)
            .or_else(|| common::union_members(self.ctx.types, resolved))
            .or_else(|| common::union_members(self.ctx.types, evaluated))
            .or_else(
                || match classify_for_excess_properties(self.ctx.types, type_id) {
                    ExcessPropertiesKind::Union(members) => Some(members.into()),
                    _ => None,
                },
            )
            .or_else(
                || match classify_for_excess_properties(self.ctx.types, resolved) {
                    ExcessPropertiesKind::Union(members) => Some(members.into()),
                    _ => None,
                },
            )
            .or_else(
                || match classify_for_excess_properties(self.ctx.types, evaluated) {
                    ExcessPropertiesKind::Union(members) => Some(members.into()),
                    _ => None,
                },
            );

        let Some(members) = members else {
            return false;
        };

        for member in members {
            if self.ctx.types.is_nullish_type(member) {
                continue;
            }
            let evaluated_member = self.evaluate_type_with_env(member);
            let evaluated_member = self.resolve_lazy_type(evaluated_member);
            let evaluated_member = self.evaluate_application_type(evaluated_member);
            let resolved_member = self.resolve_type_for_property_access(member);
            let resolved_evaluated_member = self.resolve_type_for_property_access(evaluated_member);
            let is_primitive = common::is_primitive_type(self.ctx.types, member)
                || common::is_primitive_type(self.ctx.types, evaluated_member);
            if is_primitive
                && (matches!(
                    self.resolve_property_access_with_env(member, property_name),
                    PropertyAccessResult::Success { .. }
                ) || matches!(
                    self.resolve_property_access_with_env(resolved_member, property_name),
                    PropertyAccessResult::Success { .. }
                ) || matches!(
                    self.resolve_property_access_with_env(resolved_evaluated_member, property_name),
                    PropertyAccessResult::Success { .. }
                ))
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn precise_callable_context_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let type_id = common::remove_undefined(self.ctx.types, type_id);
        if type_id == TypeId::UNDEFINED {
            return None;
        }

        if let Some(members) = common::union_members(self.ctx.types, type_id) {
            let callable_members: Vec<_> = members
                .into_iter()
                .filter(|&member| member != TypeId::UNDEFINED)
                .collect();
            if !callable_members.is_empty()
                && callable_members
                    .iter()
                    .all(|&member| common::is_callable_type(self.ctx.types, member))
            {
                return Some(object_context_query::contextual_union_preserve_members(
                    self.ctx.types,
                    callable_members,
                ));
            }
            return None;
        }

        if let Some(members) = common::intersection_members(self.ctx.types, type_id) {
            let callable_members: Vec<_> = members
                .into_iter()
                .filter(|&member| common::is_callable_type(self.ctx.types, member))
                .collect();
            return match callable_members.as_slice() {
                [] => None,
                [single] => Some(*single),
                _ => Some(object_context_query::contextual_intersection(
                    self.ctx.types,
                    callable_members,
                )),
            };
        }

        common::is_callable_type(self.ctx.types, type_id).then_some(type_id)
    }

    fn callable_context_type_from_mixed_union(&mut self, type_id: TypeId) -> Option<TypeId> {
        let type_id = common::remove_undefined(self.ctx.types, type_id);
        if type_id == TypeId::UNDEFINED {
            return None;
        }

        if common::is_callable_type(self.ctx.types, type_id) {
            return Some(type_id);
        }

        let members = common::union_members(self.ctx.types, type_id)?;

        let callable_members: Vec<_> = members
            .into_iter()
            .filter_map(|member| {
                if common::is_callable_type(self.ctx.types, member) {
                    return Some(member);
                }
                let complexity_checkpoint = self.ctx.types.union_complexity_checkpoint();
                let evaluated = self.evaluate_type_with_env(member);
                let evaluated = self.resolve_lazy_type(evaluated);
                let evaluated = self.evaluate_application_type(evaluated);
                if self
                    .ctx
                    .types
                    .take_union_too_complex_since(complexity_checkpoint)
                {
                    return None;
                }
                common::is_callable_type(self.ctx.types, evaluated).then_some(evaluated)
            })
            .collect();

        match callable_members.len() {
            0 => None,
            1 => Some(callable_members[0]),
            _ => Some(object_context_query::contextual_union_preserve_members(
                self.ctx.types,
                callable_members,
            )),
        }
    }

    pub(crate) fn function_initializer_context_type(
        &mut self,
        contextual_type: Option<TypeId>,
        property_name: &str,
        property_context_type: Option<TypeId>,
        initializer_idx: NodeIndex,
    ) -> Option<TypeId> {
        let property_context_type = property_context_type?;
        let Some(initializer_node) = self.ctx.arena.get(initializer_idx) else {
            return Some(property_context_type);
        };

        if initializer_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && initializer_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return Some(property_context_type);
        }

        if contextual_type.is_some_and(|ctx_type| {
            self.primitive_union_member_has_property(ctx_type, property_name)
        }) {
            return None;
        }

        if let Some(callable_only) =
            self.callable_context_type_from_mixed_union(property_context_type)
        {
            return Some(callable_only);
        }

        if !common::type_contains_undefined(self.ctx.types, property_context_type) {
            return Some(property_context_type);
        }

        // TS7006 rule: when the outer contextual type is a union that includes a non-nullish
        // non-object member (e.g., `string` in `string | FullRule`), do not provide a
        // contextual type for function properties. Without this, the parameter would get
        // the type from the object-union member (suppressing the TS7006 implicit-any error).
        // This check must come before the property-access refinement below.
        if contextual_type
            .is_some_and(|ctx_type| self.union_with_non_nullish_non_object_member(ctx_type, 6))
        {
            return None;
        }

        if let Some(contextual_type) = contextual_type
            && let tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id, ..
            } = self.resolve_property_access_with_env(contextual_type, property_name)
            && let Some(type_id) = self.precise_callable_context_type(type_id)
        {
            return self
                .prefer_more_specific_contextual_property_type(Some(type_id), property_context_type)
                .or(Some(type_id));
        }

        if self
            .precise_callable_context_type(property_context_type)
            .is_some_and(|type_id| type_id != property_context_type)
        {
            return self.precise_callable_context_type(property_context_type);
        }

        let Some(contextual_type) = contextual_type else {
            return Some(property_context_type);
        };

        if self.union_with_non_nullish_non_object_member(contextual_type, 6) {
            None
        } else {
            Some(property_context_type)
        }
    }

    fn contextual_property_presence(
        &mut self,
        type_id: TypeId,
        property_name: &str,
        depth: usize,
    ) -> ContextualPropertyPresence {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;
        use common::PropertyAccessResult;

        let type_id = common::remove_undefined(self.ctx.types, type_id);
        if type_id == TypeId::UNDEFINED {
            return ContextualPropertyPresence::Absent;
        }
        if depth == 0 || matches!(type_id, TypeId::ANY | TypeId::ERROR) {
            return ContextualPropertyPresence::Unknown;
        }

        match self.resolve_property_access_with_env(type_id, property_name) {
            PropertyAccessResult::Success { .. } => return ContextualPropertyPresence::Present,
            PropertyAccessResult::PropertyNotFound { .. } => {}
            _ => return ContextualPropertyPresence::Unknown,
        }

        let resolved_type = self.resolve_type_for_property_access(type_id);
        if resolved_type != type_id {
            match self.contextual_property_presence(resolved_type, property_name, depth - 1) {
                ContextualPropertyPresence::Present => return ContextualPropertyPresence::Present,
                ContextualPropertyPresence::Unknown => return ContextualPropertyPresence::Unknown,
                ContextualPropertyPresence::Absent => {}
            }
        }

        let evaluated_type = self.evaluate_type_with_env(type_id);
        let evaluated_type = self.resolve_type_for_property_access(evaluated_type);
        let evaluated_type = self.resolve_lazy_type(evaluated_type);
        let evaluated_type = self.evaluate_application_type(evaluated_type);
        if evaluated_type != type_id && evaluated_type != resolved_type {
            match self.contextual_property_presence(evaluated_type, property_name, depth - 1) {
                ContextualPropertyPresence::Present => return ContextualPropertyPresence::Present,
                ContextualPropertyPresence::Unknown => return ContextualPropertyPresence::Unknown,
                ContextualPropertyPresence::Absent => {}
            }
        }

        if let Some(members) = common::intersection_members(self.ctx.types, type_id) {
            let mut saw_unknown = false;
            for member in members {
                match self.contextual_property_presence(member, property_name, depth - 1) {
                    ContextualPropertyPresence::Present => {
                        return ContextualPropertyPresence::Present;
                    }
                    ContextualPropertyPresence::Unknown => saw_unknown = true,
                    ContextualPropertyPresence::Absent => {}
                }
            }
            if saw_unknown {
                return ContextualPropertyPresence::Unknown;
            }
        }

        match crate::query_boundaries::assignability::classify_for_excess_properties(
            self.ctx.types,
            type_id,
        ) {
            ExcessPropertiesKind::Object(_) => ContextualPropertyPresence::Absent,
            ExcessPropertiesKind::ObjectWithIndex(_) => ContextualPropertyPresence::Present,
            ExcessPropertiesKind::Union(members) | ExcessPropertiesKind::Intersection(members) => {
                let mut saw_unknown = false;
                for member in members {
                    match self.contextual_property_presence(member, property_name, depth - 1) {
                        ContextualPropertyPresence::Present => {
                            return ContextualPropertyPresence::Present;
                        }
                        ContextualPropertyPresence::Unknown => saw_unknown = true,
                        ContextualPropertyPresence::Absent => {}
                    }
                }
                if saw_unknown {
                    ContextualPropertyPresence::Unknown
                } else {
                    ContextualPropertyPresence::Absent
                }
            }
            ExcessPropertiesKind::NotObject => {
                if common::contains_type_parameters(self.ctx.types, type_id) {
                    ContextualPropertyPresence::Unknown
                } else {
                    ContextualPropertyPresence::Absent
                }
            }
        }
    }

    pub(crate) fn check_destructuring_default_initializer(
        &mut self,
        default_idx: NodeIndex,
        target_type: TypeId,
        diag_idx: NodeIndex,
    ) {
        if default_idx.is_none() {
            return;
        }

        let request = if target_type != TypeId::ANY
            && target_type != TypeId::NEVER
            && target_type != TypeId::UNKNOWN
            && !self.type_contains_error(target_type)
        {
            match self.contextual_type_option_for_expression(Some(target_type)) {
                Some(ctx_ty) => crate::context::TypingRequest::with_contextual_type(ctx_ty),
                None => crate::context::TypingRequest::NONE,
            }
        } else {
            crate::context::TypingRequest::NONE
        };
        let default_type = self.get_type_of_node_with_request(default_idx, &request);

        if target_type != TypeId::ANY
            && target_type != TypeId::NEVER
            && target_type != TypeId::UNKNOWN
            && !self.type_contains_error(target_type)
        {
            // Nested assignment patterns are validated as the pattern is walked.
            // A whole-pattern default check here is too eager and rejects valid
            // array/tuple defaults used through numeric property destructuring.
            if self.ctx.arena.get(diag_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            }) {
                return;
            }

            let literal_source = self.literal_type_from_initializer(default_idx);
            let source_type = literal_source.unwrap_or(default_type);
            let source_for_display = literal_source
                .map(|ty| self.widen_literal_type(ty))
                .filter(|&ty| ty == TypeId::NUMBER)
                .unwrap_or(source_type);
            let _ = self.check_assignable_or_report_at_with_display_types(
                source_type,
                target_type,
                source_for_display,
                target_type,
                default_idx,
                diag_idx,
            );
        }
    }

    pub(crate) fn destructuring_target_type_from_initializer(
        &mut self,
        init_idx: NodeIndex,
    ) -> TypeId {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return TypeId::ANY;
        };

        if init_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(init_node)
            && bin.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
        {
            let target_type = self.get_type_of_assignment_target(bin.left);
            self.check_destructuring_default_initializer(bin.right, target_type, bin.left);
            return target_type;
        }

        self.get_type_of_assignment_target(init_idx)
    }

    pub(crate) fn contextual_object_literal_property_type(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let contextual_type = self.strip_contextual_this_type_markers(contextual_type);
        let union_member_property_type = |this: &mut Self,
                                          union_type: TypeId,
                                          property_name: &str|
         -> Option<TypeId> {
            let members = common::union_members(this.ctx.types, union_type).or_else(|| {
                match crate::query_boundaries::assignability::classify_for_excess_properties(
                    this.ctx.types,
                    union_type,
                ) {
                    crate::query_boundaries::assignability::ExcessPropertiesKind::Union(
                        members,
                    ) => Some(members.into()),
                    _ => None,
                }
            })?;
            let mut property_types = Vec::new();
            let mut has_unresolved_member = false;

            for &member in &members {
                let resolved_member = this.resolve_type_for_property_access(member);
                let evaluated_member = this.evaluate_type_with_env(member);
                let evaluated_member_for_property_access =
                    this.resolve_type_for_property_access(evaluated_member);
                let evaluated_member_for_property_access =
                    this.resolve_lazy_type(evaluated_member_for_property_access);
                let evaluated_member_for_property_access =
                    this.evaluate_application_type(evaluated_member_for_property_access);
                if call_checker::is_type_parameter_type(this.ctx.types, member)
                    || call_checker::is_type_parameter_type(this.ctx.types, resolved_member)
                    || call_checker::is_type_parameter_type(
                        this.ctx.types,
                        evaluated_member_for_property_access,
                    )
                    || this.contextual_type_is_unresolved_for_argument_refresh(member)
                    || this.contextual_type_is_unresolved_for_argument_refresh(resolved_member)
                    || this.contextual_type_is_unresolved_for_argument_refresh(
                        evaluated_member_for_property_access,
                    )
                {
                    has_unresolved_member = true;
                }
                let mut property_type = this
                    .ctx
                    .types
                    .contextual_property_type(member, property_name);

                // When the property type is `any`, it may come from an index signature
                // in an intersection with unresolved Lazy members (e.g.,
                // `Lazy(Interface) & { [k: string]: any }`). Try the resolved paths
                // which can evaluate Lazy types to get the specific property type.
                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this
                        .ctx
                        .types
                        .contextual_property_type(resolved_member, property_name)
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this.ctx.types.contextual_property_type(
                        evaluated_member_for_property_access,
                        property_name,
                    )
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                let mut alternate_member_for_property_access = None;
                if property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY) {
                    use crate::query_boundaries::state::type_environment::evaluate_type_with_resolver;

                    let alternate_member =
                        evaluate_type_with_resolver(this.ctx.types, &this.ctx, member);
                    let alternate_member = this.resolve_type_for_property_access(alternate_member);
                    let alternate_member = this.resolve_lazy_type(alternate_member);
                    let alternate_member = this.evaluate_application_type(alternate_member);
                    alternate_member_for_property_access = Some(alternate_member);
                    property_type = this
                        .ctx
                        .types
                        .contextual_property_type(alternate_member, property_name);
                }

                let property_type = property_type;
                if property_type.is_none() {
                    tracing::trace!(
                        union_type = union_type.0,
                        union_type_str = %this.format_type(union_type),
                        property_name,
                        member = member.0,
                        member_str = %this.format_type(member),
                        resolved_member = resolved_member.0,
                        resolved_member_str = %this.format_type(resolved_member),
                        evaluated_member = evaluated_member.0,
                        evaluated_member_str = %this.format_type(evaluated_member),
                        evaluated_member_for_property_access = evaluated_member_for_property_access.0,
                        evaluated_member_for_property_access_str = %this.format_type(evaluated_member_for_property_access),
                        alternate_member_for_property_access = alternate_member_for_property_access.map(|id| id.0),
                        alternate_member_for_property_access_str = alternate_member_for_property_access
                            .map(|id| this.format_type(id))
                            .unwrap_or_default(),
                        "contextual_object_literal_property_type: union-member miss"
                    );
                }
                if let Some(property_type) = property_type {
                    property_types.push(property_type);
                }
            }

            if property_types.is_empty() {
                has_unresolved_member.then_some(TypeId::ANY)
            } else {
                Some(object_context_query::contextual_union_preserve_members(
                    this.ctx.types,
                    property_types,
                ))
            }
        };
        let intersection_member_property_type = |this: &mut Self,
                                                 intersection_type: TypeId,
                                                 property_name: &str|
         -> Option<TypeId> {
            let members = common::intersection_members(this.ctx.types, intersection_type)?;
            let mut property_types = Vec::new();

            for &member in &members {
                let resolved_member = this.resolve_type_for_property_access(member);
                let evaluated_member = this.evaluate_type_with_env(member);
                let evaluated_member_for_property_access =
                    this.resolve_type_for_property_access(evaluated_member);
                let evaluated_member_for_property_access =
                    this.resolve_lazy_type(evaluated_member_for_property_access);
                let evaluated_member_for_property_access =
                    this.evaluate_application_type(evaluated_member_for_property_access);
                let mut property_type = this
                    .ctx
                    .types
                    .contextual_property_type(member, property_name);

                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this
                        .ctx
                        .types
                        .contextual_property_type(resolved_member, property_name)
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                if (property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY))
                    && let Some(pt) = this.ctx.types.contextual_property_type(
                        evaluated_member_for_property_access,
                        property_name,
                    )
                    && (pt != tsz_solver::TypeId::ANY || property_type.is_none())
                {
                    property_type = Some(pt);
                }

                let mut _alternate_member_for_property_access = None;
                if property_type.is_none() || property_type == Some(tsz_solver::TypeId::ANY) {
                    use crate::query_boundaries::state::type_environment::evaluate_type_with_resolver;

                    let alternate_member =
                        evaluate_type_with_resolver(this.ctx.types, &this.ctx, member);
                    let alternate_member = this.resolve_type_for_property_access(alternate_member);
                    let alternate_member = this.resolve_lazy_type(alternate_member);
                    let alternate_member = this.evaluate_application_type(alternate_member);
                    _alternate_member_for_property_access = Some(alternate_member);
                    property_type = this
                        .ctx
                        .types
                        .contextual_property_type(alternate_member, property_name);
                }

                if let Some(property_type) = property_type {
                    property_types.push(property_type);
                }
            }

            if property_types.is_empty() {
                None
            } else {
                Some(object_context_query::contextual_union_preserve_members(
                    this.ctx.types,
                    property_types,
                ))
            }
        };
        let original_contextual_type = contextual_type;
        let mut best_property_type = None;
        let env_property_type = if matches!(
            self.resolve_property_access_with_env(original_contextual_type, property_name),
            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
        ) {
            match self.resolve_property_access_with_env(original_contextual_type, property_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => Some(type_id),
                _ => None,
            }
        } else {
            None
        };

        if let Some(constraint) =
            common::type_parameter_constraint(self.ctx.types, original_contextual_type)
            && constraint != original_contextual_type
            && let Some(property_type) =
                self.contextual_object_literal_property_type(constraint, property_name)
        {
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(property_type) =
            self.mapped_contextual_property_type(original_contextual_type, property_name)
        {
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(env_property_type) = env_property_type {
            best_property_type = self.prefer_more_specific_contextual_property_type(
                best_property_type,
                env_property_type,
            );
        }

        // Skip the un-resolved contextual extraction for a non-identity
        // homomorphic mapped application: the solver's resolver-less fallback
        // would read the property off the source type argument, dropping the
        // mapped template's modifiers and producing a wrongly-narrowed type that
        // then wins `prefer_more_specific`. The fully-resolved form below
        // supplies the correct property type for these targets.
        if !self.application_alias_body_is_non_identity_mapped(original_contextual_type)
            && let Some(property_type) = self
                .ctx
                .types
                .contextual_property_type(original_contextual_type, property_name)
        {
            // When the property type is `any`, it may come from an index signature
            // in a distributed intersection. Don't return eagerly — fall through
            // to resolved paths which can extract the specific property type.
            if property_type != tsz_solver::TypeId::ANY {
                tracing::trace!(
                    contextual_type = original_contextual_type.0,
                    property_name,
                    property_type = property_type.0,
                    "contextual_object_literal_property_type: pre-eval extracted"
                );
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    property_type,
                );
            }
        }

        if let Some(property_type) =
            union_member_property_type(self, original_contextual_type, property_name)
        {
            tracing::trace!(
                contextual_type = original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: union-member extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(property_type) =
            intersection_member_property_type(self, original_contextual_type, property_name)
        {
            tracing::trace!(
                contextual_type = original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: intersection-member extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        let resolved_original_contextual_type =
            self.resolve_type_for_property_access(original_contextual_type);
        if resolved_original_contextual_type != original_contextual_type
            && !self
                .application_alias_body_is_non_identity_mapped(resolved_original_contextual_type)
            && let Some(property_type) = self
                .ctx
                .types
                .contextual_property_type(resolved_original_contextual_type, property_name)
        {
            tracing::trace!(
                original_contextual_type = original_contextual_type.0,
                resolved_original_contextual_type = resolved_original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: resolved-original extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if resolved_original_contextual_type != original_contextual_type
            && let Some(property_type) =
                union_member_property_type(self, resolved_original_contextual_type, property_name)
        {
            tracing::trace!(
                original_contextual_type = original_contextual_type.0,
                resolved_original_contextual_type = resolved_original_contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: resolved-union-member extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        // Cache the expensive contextual type resolution chain.
        // The same contextual type is resolved for each property of an object literal,
        // so caching saves O(properties-1) full resolution chains per literal.
        let contextual_type = if let Some(&cached) = self
            .ctx
            .flow_shared
            .narrowing_cache
            .contextual_resolve_cache
            .borrow()
            .get(&original_contextual_type)
        {
            cached
        } else {
            let ct = self.evaluate_contextual_type(contextual_type);
            let ct = self.evaluate_type_with_env(ct);
            let ct = self.resolve_type_for_property_access(ct);
            let ct = self.resolve_lazy_type(ct);
            let ct = self.evaluate_application_type(ct);
            self.ctx
                .flow_shared
                .narrowing_cache
                .contextual_resolve_cache
                .borrow_mut()
                .insert(original_contextual_type, ct);
            ct
        };

        if contextual_type == TypeId::UNKNOWN {
            return Some(best_property_type.unwrap_or(TypeId::UNKNOWN));
        }

        if contextual_type != original_contextual_type
            && let Some(property_type) =
                self.mapped_contextual_property_type(contextual_type, property_name)
        {
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(property_type) = self
            .ctx
            .types
            .contextual_property_type(contextual_type, property_name)
        {
            tracing::trace!(
                contextual_type = contextual_type.0,
                property_name,
                property_type = property_type.0,
                "contextual_object_literal_property_type: extracted"
            );
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

        if let Some(type_id) = env_property_type {
            tracing::trace!(
                contextual_type = contextual_type.0,
                property_name,
                property_type = type_id.0,
                "contextual_object_literal_property_type: env property access extracted"
            );
            best_property_type =
                self.prefer_more_specific_contextual_property_type(best_property_type, type_id);
        }

        let alternate_contextual_type = {
            use crate::query_boundaries::state::type_environment::evaluate_type_with_resolver;
            evaluate_type_with_resolver(self.ctx.types, &self.ctx, original_contextual_type)
        };
        if alternate_contextual_type != contextual_type {
            let alternate_contextual_type =
                self.resolve_type_for_property_access(alternate_contextual_type);
            let alternate_contextual_type = self.resolve_lazy_type(alternate_contextual_type);
            let alternate_contextual_type =
                self.evaluate_application_type(alternate_contextual_type);
            if let Some(property_type) = self
                .ctx
                .types
                .contextual_property_type(alternate_contextual_type, property_name)
            {
                tracing::trace!(
                    original_contextual_type = original_contextual_type.0,
                    alternate_contextual_type = alternate_contextual_type.0,
                    property_name,
                    property_type = property_type.0,
                    "contextual_object_literal_property_type: alternate extracted"
                );
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    property_type,
                );
            }
        }

        let property_presence =
            self.contextual_property_presence(original_contextual_type, property_name, 6);
        let resolved_property_presence = if contextual_type != original_contextual_type {
            self.contextual_property_presence(contextual_type, property_name, 4)
        } else {
            ContextualPropertyPresence::Unknown
        };
        let effective_property_presence = match resolved_property_presence {
            ContextualPropertyPresence::Present | ContextualPropertyPresence::Absent => {
                resolved_property_presence
            }
            ContextualPropertyPresence::Unknown => property_presence,
        };
        if effective_property_presence == ContextualPropertyPresence::Absent
            && !self.should_preserve_absent_contextual_property_type(original_contextual_type, 6)
        {
            best_property_type = None;
        }

        if let Some(property_type) = best_property_type {
            let property_type = self.sanitize_contextual_property_type(property_type);
            // Under `exactOptionalPropertyTypes`, a *present* value for an
            // optional property (`y?: number`) is contextually typed against
            // the bare declared type (`number`), not the read-side type with
            // `undefined` unioned in. This only changes anything when the
            // property is sugar-optional with no explicit `undefined` in its
            // own type — `y?: number | undefined` still contextually types
            // as `number | undefined`, matching `tsc`.
            //
            // `get_property_assignment_type` (like `get_property_type`) does
            // not resolve a `Lazy(DefId)` interface/type-alias reference on
            // its own, so a named target (`interface S { y?: number }`,
            // `const s: S = ...`) needs the checker-resolved `contextual_type`
            // below rather than the raw `original_contextual_type` — an
            // inline object contextual type (a call parameter's `{ y?: T }`)
            // is already concrete and answers directly. Try
            // `original_contextual_type` first and only fall back to the
            // resolved `contextual_type` when it has no answer at all: falling
            // back unconditionally risks a *different* resolved form (e.g. a
            // fresh re-evaluation through a call-argument contextual type)
            // silently overriding an already-correct direct answer — that
            // regressed the explicit-`y?: number | undefined` control case
            // during review.
            if self.ctx.exact_optional_property_types()
                && let Some(assignment_type) = self
                    .ctx
                    .types
                    .contextual_property_assignment_type(original_contextual_type, property_name)
                    .or_else(|| {
                        self.ctx
                            .types
                            .contextual_property_assignment_type(contextual_type, property_name)
                    })
                && assignment_type != property_type
            {
                return Some(self.sanitize_contextual_property_type(assignment_type));
            }
            return Some(property_type);
        }

        // If contextual extraction fails but the parent context is generic/deferred,
        // preserve an `unknown` contextual slot to prevent false implicit-any
        // diagnostics during higher-order inference rounds.
        if common::contains_type_parameters(self.ctx.types, contextual_type)
            && effective_property_presence != ContextualPropertyPresence::Absent
        {
            tracing::trace!(
                original_contextual_type = original_contextual_type.0,
                contextual_type = contextual_type.0,
                property_name,
                "contextual_object_literal_property_type: deferred unknown"
            );
            return Some(TypeId::UNKNOWN);
        }

        tracing::trace!(
            original_contextual_type = original_contextual_type.0,
            original_contextual_type_str = %self.format_type(original_contextual_type),
            contextual_type = contextual_type.0,
            contextual_type_str = %self.format_type(contextual_type),
            property_name,
            "contextual_object_literal_property_type: no property type"
        );
        None
    }

    /// Whether `type_id` is a generic alias application whose alias body is a
    /// mapped type that is **not** identity homomorphic (`{ [K in keyof T]: T[K] }`).
    ///
    /// For such a target (e.g. `Outer<T> = { [K in keyof T]?: Partial<T[K]> }`),
    /// the resolver-less contextual property extraction's "read the property
    /// directly off the first type argument" shortcut is unsound: it discards
    /// the mapped template and the inner mapped's optional/readonly modifiers,
    /// yielding a wrongly-narrowed property type (`{ b: number }` instead of
    /// `{ b?: number }`). Because the narrower type wins the
    /// `prefer_more_specific` comparison, it would override the correct,
    /// fully-resolved property type. This predicate lets the un-resolved
    /// extraction be skipped so the resolved application form stays
    /// authoritative.
    ///
    /// Identity homomorphic aliases (`Partial`, `Readonly`, `Required`, an
    /// `Id`-style passthrough — all with template `T[K]`) are intentionally not
    /// matched: their result property *value* type equals the source property
    /// type, so the shortcut remains correct for them (the optionality they add
    /// is a property-level modifier, not a change to the value type).
    fn application_alias_body_is_non_identity_mapped(&mut self, type_id: TypeId) -> bool {
        let Some((base, _)) = common::application_info(self.ctx.types, type_id) else {
            return false;
        };
        let body = self.resolve_lazy_type(base);
        common::mapped_type_id(self.ctx.types, body).is_some_and(|mapped_id| {
            crate::query_boundaries::common::classify_identity_mapped(self.ctx.types, mapped_id)
                .is_none()
        })
    }

    fn mapped_contextual_property_type(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let mapped = common::mapped_type_info(self.ctx.types, contextual_type)?;
        let constraint = mapped.constraint;
        let template = mapped.template;

        let numeric_key = property_name
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite());
        let key_type = if let Some(value) = numeric_key {
            object_context_query::mapped_contextual_property_number_key_type(self.ctx.types, value)
        } else {
            object_context_query::mapped_contextual_property_string_key_type(
                self.ctx.types,
                property_name,
            )
        };

        let constraint_resolved = self.resolve_lazy_type(constraint);
        let constraint_evaluated = self.evaluate_type_for_assignability(constraint_resolved);
        if self
            .object_literal_mapped_contextual_key_relation_outcome(key_type, constraint_resolved)
            .related
            || self
                .object_literal_mapped_contextual_key_relation_outcome(
                    key_type,
                    constraint_evaluated,
                )
                .related
        {
            let mut substitution = TypeSubstitution::new();
            substitution.insert(mapped.type_param.name, key_type);
            let instantiated = common::instantiate_type(self.ctx.types, template, &substitution);
            let evaluated = self.evaluate_type_with_env(instantiated);
            let evaluated = self.resolve_lazy_type(evaluated);
            return Some(self.evaluate_application_type(evaluated));
        }

        if numeric_key.is_some() && common::contains_type_parameters(self.ctx.types, constraint) {
            let mut substitution = TypeSubstitution::new();
            substitution.insert(mapped.type_param.name, key_type);
            let instantiated = common::instantiate_type(self.ctx.types, template, &substitution);
            let evaluated = self.evaluate_type_with_env(instantiated);
            let evaluated = self.resolve_lazy_type(evaluated);
            return Some(self.evaluate_application_type(evaluated));
        }

        None
    }

    /// True when `union` collapses to `member` as a set: `member` is one of its
    /// arms (checked by the caller) and every arm is assignable to `member`, so
    /// `union` and `member` denote the same value set (`2 | number` vs
    /// `number`). The un-reduced `union` is then the literal-preserving
    /// contextual form (`UnionReduction.None`) and must not be collapsed to the
    /// bare `member`. When `union` is *not* assignable to `member` it is
    /// genuinely wider (`string | number` vs `number`) and the narrower member
    /// stays preferred.
    ///
    /// Callable operands are excluded: literal preservation is about data
    /// unions, while a union of overloaded call signatures is collapsed to a
    /// specific member by the dedicated callable-specificity comparison that
    /// runs immediately after the membership rules. Keeping a callable union
    /// here would steal that decision and drop the precise signature used to
    /// contextually type a method/callback property (`then`, `set`, …).
    fn contextual_union_reduces_to_member(&self, union: TypeId, member: TypeId) -> bool {
        let union_eval = common::evaluate_type(self.ctx.types, union);
        let member_eval = common::evaluate_type(self.ctx.types, member);
        if crate::query_boundaries::common::is_callable_type(self.ctx.types, union_eval)
            || crate::query_boundaries::common::is_callable_type(self.ctx.types, member_eval)
        {
            return false;
        }
        crate::query_boundaries::assignability::is_fresh_subtype_of(
            self.ctx.types,
            union_eval,
            member_eval,
        )
    }

    fn prefer_more_specific_contextual_property_type(
        &self,
        current: Option<TypeId>,
        candidate: TypeId,
    ) -> Option<TypeId> {
        let Some(current) = current else {
            return Some(candidate);
        };

        if current == candidate {
            return Some(current);
        }

        if current == TypeId::NEVER && candidate != TypeId::NEVER {
            return Some(candidate);
        }
        if candidate == TypeId::NEVER && current != TypeId::NEVER {
            return Some(current);
        }

        if matches!(current, TypeId::ANY | TypeId::UNKNOWN)
            && !matches!(candidate, TypeId::ANY | TypeId::UNKNOWN)
        {
            return Some(candidate);
        }
        if matches!(candidate, TypeId::ANY | TypeId::UNKNOWN)
            && !matches!(current, TypeId::ANY | TypeId::UNKNOWN)
        {
            return Some(current);
        }

        // One operand being a union that *contains* the other as a member is
        // usually a signal that the lone member is the more specific contextual
        // type. But when the union reduces to exactly that member as a set —
        // every arm is assignable to it, e.g. `2 | number` collapses to
        // `number` — the un-reduced union is the literal-preserving form tsc
        // keeps under `UnionReduction.None`. Collapsing to the bare member there
        // drops the literal arm and widens a fresh property/element, so a literal
        // object/array assigned to a differing-arity union (`{ k: 2 }` to
        // `{ k: 2 } | { k: number; j: boolean }`) loses the arm it matched and
        // reports a spurious TS2322. Keep the union in that case; only collapse
        // when the member is a *strict* subset (the union is genuinely wider).
        // Checked in both orderings, since either operand may be the union.
        for (union, member) in [(current, candidate), (candidate, current)] {
            if common::union_members(self.ctx.types, union)
                .is_some_and(|members| members.contains(&member))
            {
                return Some(if self.contextual_union_reduces_to_member(union, member) {
                    union
                } else {
                    member
                });
            }
        }

        if common::intersection_members(self.ctx.types, current)
            .is_some_and(|members| members.contains(&candidate))
        {
            return Some(candidate);
        }
        if common::intersection_members(self.ctx.types, candidate)
            .is_some_and(|members| members.contains(&current))
        {
            return Some(current);
        }

        if let Some(preferred) =
            self.prefer_more_specific_callable_contextual_type(current, candidate)
        {
            return Some(preferred);
        }

        let current_eval = common::evaluate_type(self.ctx.types, current);
        let candidate_eval = common::evaluate_type(self.ctx.types, candidate);
        let candidate_narrower = crate::query_boundaries::assignability::is_fresh_subtype_of(
            self.ctx.types,
            candidate_eval,
            current_eval,
        );
        let current_narrower = crate::query_boundaries::assignability::is_fresh_subtype_of(
            self.ctx.types,
            current_eval,
            candidate_eval,
        );

        if candidate_narrower && !current_narrower {
            Some(candidate)
        } else {
            Some(current)
        }
    }

    fn prefer_more_specific_callable_contextual_type(
        &self,
        current: TypeId,
        candidate: TypeId,
    ) -> Option<TypeId> {
        let current_ctx = ContextualTypeContext::with_expected(self.ctx.types, current);
        let candidate_ctx = ContextualTypeContext::with_expected(self.ctx.types, candidate);

        let mut prefer_current = false;
        let mut prefer_candidate = false;
        let mut saw_callable_params = false;

        for index in 0..8 {
            let current_param = current_ctx.get_parameter_type(index);
            let candidate_param = candidate_ctx.get_parameter_type(index);

            match (current_param, candidate_param) {
                (None, None) => break,
                (Some(_), None) | (None, Some(_)) => return None,
                (Some(current_param), Some(candidate_param)) => {
                    saw_callable_params = true;
                    if current_param == candidate_param {
                        continue;
                    }

                    let current_eval = common::evaluate_type(self.ctx.types, current_param);
                    let candidate_eval = common::evaluate_type(self.ctx.types, candidate_param);
                    let current_narrower =
                        crate::query_boundaries::assignability::is_fresh_subtype_of(
                            self.ctx.types,
                            current_eval,
                            candidate_eval,
                        );
                    let candidate_narrower =
                        crate::query_boundaries::assignability::is_fresh_subtype_of(
                            self.ctx.types,
                            candidate_eval,
                            current_eval,
                        );

                    if current_narrower && !candidate_narrower {
                        prefer_current = true;
                    } else if candidate_narrower && !current_narrower {
                        prefer_candidate = true;
                    }
                }
            }
        }

        if !saw_callable_params || prefer_current == prefer_candidate {
            None
        } else if prefer_current {
            Some(current)
        } else {
            Some(candidate)
        }
    }

    /// Narrow a union contextual type by inspecting discriminant properties in the
    /// object literal.  When the object literal has properties with literal values
    /// (e.g. `kind: "a"`) that match only a subset of the union members, we narrow
    /// the contextual type so that other properties receive precise contextual types
    /// from the matching member(s) rather than a union of all members' property types.
    ///
    /// This is how tsc provides precise contextual typing for discriminated union
    /// object literals:
    /// ```ts
    /// type A = { kind: "a"; onClick: (e: string) => void };
    /// type B = { kind: "b"; onClick: (e: number) => void };
    /// const x: A | B = { kind: "a", onClick: (e) => e.length }; // e: string
    /// ```
    /// Returns `true` when the syntactic form of `idx` can never have a unit
    /// (literal) type, so type-checking it to look for a discriminant value is
    /// pure overhead — and, worse, would commit context-free diagnostics for a
    /// node that is re-checked later with its proper contextual type.
    ///
    /// Covers function/arrow expressions (always a function type), object/array
    /// literals (always an object/array type), and class/JSX expressions. These
    /// already classify as non-unit today; skipping the probe only suppresses
    /// the premature side-effect diagnostics, leaving discriminant detection
    /// unchanged.
    fn initializer_is_structurally_non_unit(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        matches!(
            node.kind,
            syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                | syntax_kind_ext::CLASS_EXPRESSION
                | syntax_kind_ext::JSX_ELEMENT
                | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                | syntax_kind_ext::JSX_FRAGMENT
        )
    }

    pub(crate) fn narrow_contextual_union_via_object_literal_discriminants(
        &mut self,
        ctx_type: TypeId,
        elements: &[NodeIndex],
    ) -> TypeId {
        // Get union members; bail if not a union.
        let resolved = self.resolve_type_for_property_access(ctx_type);
        let Some(members) = common::union_members(self.ctx.types, resolved) else {
            return ctx_type;
        };
        let raw_members = common::union_members(self.ctx.types, ctx_type);

        if members.len() < 2 {
            return ctx_type;
        }

        // Pre-scan: collect discriminant info from the object literal.
        // - `unit_discriminants`: properties with unit-type literal values (e.g. `kind: "a"`)
        // - `present_property_names`: all explicitly named properties (for never-elimination)
        // - `non_unit_named_properties`: present properties whose initializer is NOT a
        //   unit literal (e.g. `type: foo1` where `foo1: string`). When such a property
        //   names a discriminator slot in the union, narrowing must bail entirely so
        //   the diagnostic reports the full union (`"foo" | "bar"`) rather than a
        //   single arm — matches tsc's `indirectDiscriminantAndExcessProperty` shape.
        let mut unit_discriminants: Vec<(String, TypeId)> = Vec::new();
        let mut present_property_names: Vec<String> = Vec::new();
        let mut non_unit_named_properties: Vec<String> = Vec::new();
        for &elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    continue;
                };
                present_property_names.push(name.clone());
                // Get the literal type of the initializer without full type computation.
                // The `get_type_of_node` fallback exists only to catch *reference*
                // initializers whose type is a unit literal (a `const` bound to a
                // literal, an enum member, an `as const`, etc.). Initializer forms
                // whose type is structurally never a unit type — function/arrow
                // expressions, object/array literals, class/JSX expressions — can
                // never be a discriminant, so the only effect of type-checking them
                // here is to commit premature, context-free diagnostics (e.g. a
                // spurious TS7006 on an object-literal method's parameters). Skip the
                // probe for those forms; the real, contextually-typed check of the
                // object-literal element still runs afterward.
                let unit_lit = self
                    .literal_type_from_initializer(prop.initializer)
                    .or_else(|| {
                        if self.initializer_is_structurally_non_unit(prop.initializer) {
                            return None;
                        }
                        let initializer_type = self.get_type_of_node(prop.initializer);
                        common::is_unit_type(self.ctx.types, initializer_type)
                            .then_some(initializer_type)
                    })
                    .filter(|&lit_type| common::is_unit_type(self.ctx.types, lit_type));
                if let Some(lit_type) = unit_lit {
                    unit_discriminants.push((name, lit_type));
                } else {
                    non_unit_named_properties.push(name);
                }
            } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name) = self.get_property_name_resolved(shorthand.name)
            {
                present_property_names.push(name.clone());
                // For shorthand properties like `{ kind }` where `const kind = "a"`,
                // resolve the identifier to its const declaration and extract the literal
                // type from the initializer. This enables discriminant narrowing for
                // shorthand properties, matching tsc behavior.
                let unit_lit = self
                    .shorthand_const_literal_type(shorthand.name)
                    .or_else(|| self.literal_type_from_initializer(shorthand.name))
                    .filter(|&lit_type| common::is_unit_type(self.ctx.types, lit_type));
                if let Some(lit_type) = unit_lit {
                    unit_discriminants.push((name, lit_type));
                } else {
                    non_unit_named_properties.push(name);
                }
            }
        }

        if unit_discriminants.is_empty() && present_property_names.is_empty() {
            return ctx_type;
        }

        let mut is_discriminator_slot = |prop_name: &str| -> bool {
            let mut unit_member_count = 0;
            for &member in &members {
                let lazy_member = self.resolve_lazy_type(member);
                let resolved_member = self.resolve_type_for_property_access(lazy_member);
                let evaluated_member = self.evaluate_contextual_type(resolved_member);
                let member_candidates = [evaluated_member, resolved_member, lazy_member, member];
                let member_prop_type = member_candidates.iter().find_map(|&candidate| {
                    self.ctx
                        .types
                        .contextual_property_type(candidate, prop_name)
                });
                let Some(member_prop_type) = member_prop_type else {
                    continue;
                };
                if !common::is_unit_type(self.ctx.types, member_prop_type) {
                    return false;
                }
                unit_member_count += 1;
            }
            unit_member_count >= 2
        };

        unit_discriminants.retain(|(prop_name, _)| is_discriminator_slot(prop_name));

        // If the literal supplies a discriminator slot with a non-unit value
        // (e.g. `type: foo1` where `foo1: string`), the user is attempting a
        // dynamic discriminator. tsc reports the assignability error against
        // the FULL union (`"foo" | "bar"`); narrowing here would collapse the
        // diagnostic to a single arm. Bail entirely in that case.
        let literal_has_dynamic_discriminator = non_unit_named_properties
            .iter()
            .any(|name| is_discriminator_slot(name));
        if literal_has_dynamic_discriminator {
            return ctx_type;
        }

        // Discriminate exactly like tsc's `discriminateTypeByDiscriminableItems`:
        // every discriminator is applied independently over the still-included
        // members. A discriminator that matches at least one included member
        // eliminates the non-matching members; one that matches NO member is
        // reverted and ignored — a failing unit literal that names no arm must
        // not kill the narrowing the other discriminants produce (tsc's
        // per-discriminator `matched` flag turning `Ternary.Maybe` back into
        // `Ternary.True`).
        //
        // Deliberate divergence from tsc's `Ternary.False` primitive
        // pre-marking: every constituent starts included here. tsc applies the
        // pre-marking only to the contextual APPARENT type, while its
        // elaboration re-derives per-property targets from the full relation
        // target; tsz's elaboration gates key on `narrowed_by_discriminant`,
        // so pre-excluding primitive arms would "narrow" a JSON-style union
        // (`string | ... | T[] | { [k: string]: T }`) with zero matching
        // discriminators and lose the outer whole-object frame (pinned by
        // `fresh_object_literal_union_array_member_drill_in_tests`). A
        // matching discriminator still eliminates primitive arms through the
        // ordinary `Maybe -> No` path below.
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Include {
            Yes,
            No,
            Maybe,
        }
        let member_candidates_by_index: Vec<[TypeId; 4]> = members
            .iter()
            .map(|&member| {
                let lazy_member = self.resolve_lazy_type(member);
                let resolved_member = self.resolve_type_for_property_access(lazy_member);
                let evaluated_member = self.evaluate_contextual_type(resolved_member);
                [evaluated_member, resolved_member, lazy_member, member]
            })
            .collect();
        let mut include: Vec<Include> = vec![Include::Yes; member_candidates_by_index.len()];
        for (prop_name, lit_type) in &unit_discriminants {
            let mut matched = false;
            for (member_index, candidates) in member_candidates_by_index.iter().enumerate() {
                if include[member_index] != Include::Yes {
                    continue;
                }
                let member_prop_type = candidates.iter().find_map(|&candidate| {
                    self.ctx
                        .types
                        .contextual_property_type(candidate, prop_name)
                });
                let related = match member_prop_type {
                    Some(target_type) => {
                        *lit_type == target_type
                            || self
                                .diagnostic_subtype_outcome(*lit_type, target_type)
                                .related
                            // For optional properties (e.g. `disc?: false`), the
                            // effective type includes `undefined`.
                            // `contextual_property_type` returns the raw declared
                            // type without `undefined`, so optionality is checked
                            // explicitly when the literal is `undefined`.
                            || (*lit_type == TypeId::UNDEFINED && {
                                let prop_name_atom = self.ctx.types.intern_string(prop_name);
                                candidates.iter().any(|&candidate| {
                                    common::find_property_in_object(
                                        self.ctx.types,
                                        candidate,
                                        prop_name_atom,
                                    )
                                    .is_some_and(|p| p.optional)
                                })
                            })
                    }
                    // The member does not expose the property at all: it cannot
                    // match this discriminator (tsc's
                    // `getTypeOfPropertyOrIndexSignatureOfType` is undefined).
                    None => false,
                };
                if related {
                    matched = true;
                } else {
                    include[member_index] = Include::Maybe;
                }
            }
            for state in include.iter_mut() {
                if *state == Include::Maybe {
                    *state = if matched { Include::No } else { Include::Yes };
                }
            }
        }

        // The structural eliminations below are separate tsc inferences
        // (present-property-typed-`never`, absent-required-discriminant); they
        // filter the discriminant-included members rather than participating in
        // the per-discriminator revert above.
        let mut matching_members: Vec<TypeId> = Vec::new();
        for (member_index, &member) in members.iter().enumerate() {
            if include[member_index] != Include::Yes {
                continue;
            }
            let member_candidates = member_candidates_by_index[member_index];

            // Check present properties: eliminate members where a present property
            // has type `never` (the member requires the property to be absent).
            // Note: `prop?: never` resolves to `undefined` via contextual typing,
            // so we check the raw property type from the object shape instead.
            let never_match = present_property_names.iter().all(|prop_name| {
                let prop_name_atom = self.ctx.types.intern_string(prop_name);
                // Look up the raw property type from the member's object shape.
                let raw_prop_type = member_candidates.iter().find_map(|&candidate| {
                    common::raw_property_type(self.ctx.types, candidate, prop_name_atom)
                });
                match raw_prop_type {
                    Some(type_id) => type_id != TypeId::NEVER,
                    // Property not in object shape; don't eliminate.
                    None => true,
                }
            });

            // Check absent required discriminants: if the member has a required
            // (non-optional) property that is NOT present in the object literal,
            // AND at least one other member either doesn't have that property or
            // has it as optional, then this member can be eliminated.
            // This handles cases like:
            //   type A = { disc: true; cb: (x: string) => void }
            //   type B = { disc?: false; cb: (x: number) => void }
            //   f({ cb: n => ... })  // disc is required in A but optional in B
            //
            // Run this check even when there are no unit-typed discriminant
            // properties present in the literal — the inference is purely
            // structural (required-vs-optional) and a missing discriminator
            // is itself a signal in tsc's discriminantPropertyInference.
            let absent_required_match = {
                let mut ok = true;
                if let Some(shape) = member_candidates
                    .iter()
                    .find_map(|&candidate| common::object_shape_for_type(self.ctx.types, candidate))
                {
                    for prop in &shape.properties {
                        if prop.optional {
                            continue;
                        }
                        let prop_name_str = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                        // Skip properties that ARE present in the object literal.
                        if present_property_names.contains(&prop_name_str) {
                            continue;
                        }
                        let member_is_array_like = member_candidates.iter().any(|&candidate| {
                            common::array_element_type(self.ctx.types, candidate).is_some()
                                || common::tuple_elements(self.ctx.types, candidate).is_some()
                                || common::object_shape_for_type(self.ctx.types, candidate)
                                    .is_some_and(|shape| {
                                        shape.number_index.is_some() || {
                                            let has_length = shape.properties.iter().any(|prop| {
                                                self.ctx.types.resolve_atom_ref(prop.name).as_ref()
                                                    == "length"
                                            });
                                            let has_array_method =
                                                shape.properties.iter().any(|prop| {
                                                    matches!(
                                                        self.ctx
                                                            .types
                                                            .resolve_atom_ref(prop.name)
                                                            .as_ref(),
                                                        "push" | "pop" | "concat" | "slice"
                                                    )
                                                });
                                            has_length && has_array_method
                                        }
                                    })
                        });
                        if !member_is_array_like
                            && !common::is_unit_type(self.ctx.types, prop.type_id)
                        {
                            continue;
                        }
                        // This member requires a property that the literal doesn't have.
                        // Check if at least one other member doesn't require it (optional or absent).
                        let some_other_doesnt_require = members.iter().any(|&other| {
                            if other == member {
                                return false;
                            }
                            let lazy_other = self.resolve_lazy_type(other);
                            let resolved_other = self.resolve_type_for_property_access(lazy_other);
                            let evaluated_other = self.evaluate_contextual_type(resolved_other);
                            let other_candidates =
                                [evaluated_other, resolved_other, lazy_other, other];
                            let other_prop = other_candidates.iter().find_map(|&candidate| {
                                common::find_property_in_object(
                                    self.ctx.types,
                                    candidate,
                                    prop.name,
                                )
                            });
                            match other_prop {
                                None => true,          // other member doesn't have it at all
                                Some(p) => p.optional, // other member has it as optional
                            }
                        });
                        if some_other_doesnt_require {
                            ok = false;
                            break;
                        }
                    }
                }
                ok
            };

            if never_match && absent_required_match {
                let raw_member = raw_members
                    .as_ref()
                    .and_then(|members| members.get(member_index))
                    .copied()
                    .unwrap_or(member);
                matching_members.push(raw_member);
            }
        }

        // Only narrow if we eliminated at least one member.
        if matching_members.is_empty() || matching_members.len() == members.len() {
            return ctx_type;
        }

        if matching_members.len() == 1 {
            matching_members[0]
        } else {
            object_context_query::contextual_union_preserve_members(
                self.ctx.types,
                matching_members,
            )
        }
    }

    /// For a shorthand property identifier (e.g., `kind` in `{ kind }`),
    /// resolve it to its declaration. If the declaration is a `const` variable
    /// with a literal initializer, return the literal type.
    fn shorthand_const_literal_type(
        &self,
        name_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        let sym_id = self.resolve_identifier_symbol_without_tracking(name_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        // Only handle VariableDeclaration nodes
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        // Check if it's a const declaration
        if !self.ctx.arena.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        self.literal_type_from_initializer(var_decl.initializer)
    }

    fn sanitize_contextual_property_type(&self, property_type: TypeId) -> TypeId {
        if property_type == TypeId::ERROR
            || common::contains_error_type(self.ctx.types, property_type)
        {
            return TypeId::UNKNOWN;
        }
        if let Some(default) = common::type_parameter_default(self.ctx.types, property_type) {
            return default;
        }
        property_type
    }
}
