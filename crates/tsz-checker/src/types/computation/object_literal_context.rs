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

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextualPropertyPresence {
    Present,
    Absent,
    Unknown,
}

impl<'a> CheckerState<'a> {
    fn is_this_type_marker_application(&self, type_id: TypeId) -> bool {
        let Some(app) = crate::query_boundaries::common::type_application(self.ctx.types, type_id)
        else {
            return false;
        };
        crate::query_boundaries::common::is_this_type(self.ctx.types, app.base)
    }

    pub(crate) fn strip_contextual_this_type_markers(&self, type_id: TypeId) -> TypeId {
        if self.is_this_type_marker_application(type_id) {
            return TypeId::UNKNOWN;
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let filtered: Vec<_> = members
                .iter()
                .copied()
                .filter(|&member| !self.is_this_type_marker_application(member))
                .collect();
            return match filtered.as_slice() {
                [] => TypeId::UNKNOWN,
                [single] => *single,
                _ if filtered.len() == members.len() => type_id,
                _ => self.ctx.types.factory().intersection(filtered),
            };
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
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
                _ => self.ctx.types.factory().union_preserve_members(remapped),
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
                candidates.extend(
                    crate::query_boundaries::common::collect_callable_property_types(
                        self.ctx.types,
                        type_id,
                    ),
                );
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
            Some(self.ctx.types.factory().union_preserve_members(candidates))
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

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_mapped_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::type_application(self.ctx.types, type_id).is_some()
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
                if tsz_solver::is_primitive_type(self.ctx.types, evaluated_type) {
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
    #[allow(dead_code)] // Reserved for contextual typing improvements
    pub(crate) fn primitive_union_member_has_property(
        &mut self,
        type_id: TypeId,
        property_name: &str,
    ) -> bool {
        use crate::query_boundaries::assignability::{
            ExcessPropertiesKind, classify_for_excess_properties,
        };
        use tsz_solver::operations::property::PropertyAccessResult;

        let evaluated = self.evaluate_type_with_env(type_id);
        let evaluated = self.resolve_lazy_type(evaluated);

        let members = match classify_for_excess_properties(self.ctx.types, evaluated) {
            ExcessPropertiesKind::Union(members) => members,
            _ => return false,
        };

        for member in members {
            if self.ctx.types.is_nullish_type(member) {
                continue;
            }
            let is_primitive = matches!(
                classify_for_excess_properties(self.ctx.types, member),
                ExcessPropertiesKind::NotObject
            );
            if is_primitive
                && matches!(
                    self.resolve_property_access_with_env(member, property_name),
                    PropertyAccessResult::Success { .. }
                )
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn precise_callable_context_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let type_id = tsz_solver::remove_undefined(self.ctx.types, type_id);
        if type_id == TypeId::UNDEFINED {
            return None;
        }

        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, type_id)
        {
            let callable_members: Vec<_> = members
                .into_iter()
                .filter(|&member| member != TypeId::UNDEFINED)
                .collect();
            if !callable_members.is_empty()
                && callable_members.iter().all(|&member| {
                    tsz_solver::type_queries::is_callable_type(self.ctx.types, member)
                })
            {
                return Some(
                    self.ctx
                        .types
                        .factory()
                        .union_preserve_members(callable_members),
                );
            }
            return None;
        }

        tsz_solver::type_queries::is_callable_type(self.ctx.types, type_id).then_some(type_id)
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

        if !tsz_solver::type_contains_undefined(self.ctx.types, property_context_type) {
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
            return Some(type_id);
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

    pub(crate) fn contextual_type_has_primitive_union_member(&mut self, type_id: TypeId) -> bool {
        self.union_with_non_nullish_non_object_member(type_id, 6)
    }

    fn contextual_property_presence(
        &mut self,
        type_id: TypeId,
        property_name: &str,
        depth: usize,
    ) -> ContextualPropertyPresence {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;
        use crate::query_boundaries::common::PropertyAccessResult;

        let type_id = tsz_solver::remove_undefined(self.ctx.types, type_id);
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

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, type_id)
        {
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
                if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, type_id) {
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

            let source_type = self
                .ctx
                .arena
                .get(self.ctx.arena.skip_parenthesized(default_idx))
                .and_then(|node| match node.kind {
                    k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
                    k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
                    _ => None,
                })
                .unwrap_or(default_type);
            let _ = self.check_assignable_or_report_at_exact_anchor(
                source_type,
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
            let members = tsz_solver::type_queries::get_union_members(this.ctx.types, union_type)
                .or_else(|| {
                match crate::query_boundaries::assignability::classify_for_excess_properties(
                    this.ctx.types,
                    union_type,
                ) {
                    crate::query_boundaries::assignability::ExcessPropertiesKind::Union(
                        members,
                    ) => Some(members),
                    _ => None,
                }
            })?;
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
                None
            } else {
                Some(
                    this.ctx
                        .types
                        .factory()
                        .union_preserve_members(property_types),
                )
            }
        };
        let intersection_member_property_type = |this: &mut Self,
                                                 intersection_type: TypeId,
                                                 property_name: &str|
         -> Option<TypeId> {
            let members = tsz_solver::type_queries::get_intersection_members(
                this.ctx.types,
                intersection_type,
            )?;
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
                Some(
                    this.ctx
                        .types
                        .factory()
                        .union_preserve_members(property_types),
                )
            }
        };
        let original_contextual_type = contextual_type;
        let mut best_property_type = None;

        if let Some(constraint) = tsz_solver::type_queries::get_type_parameter_constraint(
            self.ctx.types,
            original_contextual_type,
        ) && constraint != original_contextual_type
            && let Some(property_type) =
                self.contextual_object_literal_property_type(constraint, property_name)
        {
            best_property_type = self
                .prefer_more_specific_contextual_property_type(best_property_type, property_type);
        }

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
        if let Some(property_type) = self
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

            if let Some(env_property_type) = env_property_type {
                best_property_type = self.prefer_more_specific_contextual_property_type(
                    best_property_type,
                    env_property_type,
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
                .narrowing_cache
                .contextual_resolve_cache
                .borrow_mut()
                .insert(original_contextual_type, ct);
            ct
        };

        if contextual_type == TypeId::UNKNOWN {
            return Some(best_property_type.unwrap_or(TypeId::UNKNOWN));
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
            return Some(self.sanitize_contextual_property_type(property_type));
        }

        // If contextual extraction fails but the parent context is generic/deferred,
        // preserve an `unknown` contextual slot to prevent false implicit-any
        // diagnostics during higher-order inference rounds.
        if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, contextual_type)
            && effective_property_presence != ContextualPropertyPresence::Absent
        {
            tracing::trace!(
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

        if let Some(preferred) =
            self.prefer_more_specific_callable_contextual_type(current, candidate)
        {
            return Some(preferred);
        }

        let current_eval = tsz_solver::evaluate_type(self.ctx.types, current);
        let candidate_eval = tsz_solver::evaluate_type(self.ctx.types, candidate);
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
        let current_ctx = tsz_solver::ContextualTypeContext::with_expected(self.ctx.types, current);
        let candidate_ctx =
            tsz_solver::ContextualTypeContext::with_expected(self.ctx.types, candidate);

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

                    let current_eval = tsz_solver::evaluate_type(self.ctx.types, current_param);
                    let candidate_eval = tsz_solver::evaluate_type(self.ctx.types, candidate_param);
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
    pub(crate) fn narrow_contextual_union_via_object_literal_discriminants(
        &mut self,
        ctx_type: TypeId,
        elements: &[NodeIndex],
    ) -> TypeId {
        // Get union members; bail if not a union.
        let resolved = self.resolve_type_for_property_access(ctx_type);
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, resolved)
        else {
            return ctx_type;
        };

        if members.len() < 2 {
            return ctx_type;
        }

        // Pre-scan: collect discriminant info from the object literal.
        // - `unit_discriminants`: properties with unit-type literal values (e.g. `kind: "a"`)
        // - `present_property_names`: all explicitly named properties (for never-elimination)
        let mut unit_discriminants: Vec<(String, TypeId)> = Vec::new();
        let mut present_property_names: Vec<String> = Vec::new();
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
                if let Some(lit_type) = self.literal_type_from_initializer(prop.initializer)
                    && tsz_solver::type_queries::is_unit_type(self.ctx.types, lit_type)
                {
                    unit_discriminants.push((name, lit_type));
                }
            } else if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name) = self.get_property_name_resolved(shorthand.name)
            {
                present_property_names.push(name.clone());
                // For shorthand properties like `{ kind }` where `const kind = "a"`,
                // resolve the identifier to its const declaration and extract the literal
                // type from the initializer. This enables discriminant narrowing for
                // shorthand properties, matching tsc behavior.
                if let Some(lit_type) = self
                    .shorthand_const_literal_type(shorthand.name)
                    .or_else(|| self.literal_type_from_initializer(shorthand.name))
                    && tsz_solver::type_queries::is_unit_type(self.ctx.types, lit_type)
                {
                    unit_discriminants.push((name, lit_type));
                }
            }
        }

        if unit_discriminants.is_empty() && present_property_names.is_empty() {
            return ctx_type;
        }

        // For each union member, check if all discriminant values are compatible
        // AND no present property maps to `never` in that member.
        let mut matching_members: Vec<TypeId> = Vec::new();
        for &member in &members {
            let resolved_member = self.resolve_type_for_property_access(member);

            // Check unit-type discriminants: literal must be subtype of member's prop type.
            let unit_match = unit_discriminants.iter().all(|(prop_name, lit_type)| {
                let member_prop_type = self
                    .ctx
                    .types
                    .contextual_property_type(resolved_member, prop_name)
                    .or_else(|| self.ctx.types.contextual_property_type(member, prop_name));
                match member_prop_type {
                    Some(target_type) => {
                        if *lit_type == target_type || self.is_subtype_of(*lit_type, target_type) {
                            return true;
                        }
                        // For optional properties (e.g. `disc?: false`), the effective type
                        // includes `undefined`. contextual_property_type returns the raw
                        // declared type without `undefined`, so we must check optionality
                        // explicitly. If the property is optional and the literal is
                        // `undefined`, it matches (undefined is always valid for optional
                        // properties).
                        if *lit_type == TypeId::UNDEFINED {
                            let prop_name_atom = self.ctx.types.intern_string(prop_name);
                            let is_optional =
                                crate::query_boundaries::common::find_property_in_object(
                                    self.ctx.types,
                                    resolved_member,
                                    prop_name_atom,
                                )
                                .is_some_and(|p| p.optional);
                            if is_optional {
                                return true;
                            }
                        }
                        false
                    }
                    // If the member doesn't have this property, it could still match
                    // (the property might be optional or absent).
                    None => true,
                }
            });

            // Check present properties: eliminate members where a present property
            // has type `never` (the member requires the property to be absent).
            // Note: `prop?: never` resolves to `undefined` via contextual typing,
            // so we check the raw property type from the object shape instead.
            let never_match = present_property_names.iter().all(|prop_name| {
                let prop_name_atom = self.ctx.types.intern_string(prop_name);
                // Look up the raw property type from the member's object shape.
                let raw_prop_type = crate::query_boundaries::common::raw_property_type(
                    self.ctx.types,
                    resolved_member,
                    prop_name_atom,
                );
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
            let absent_required_match = {
                let mut ok = true;
                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    resolved_member,
                ) {
                    for prop in &shape.properties {
                        if prop.optional {
                            continue;
                        }
                        let prop_name_str = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                        // Skip properties that ARE present in the object literal.
                        if present_property_names.contains(&prop_name_str) {
                            continue;
                        }
                        // This member requires a property that the literal doesn't have.
                        // Check if at least one other member doesn't require it (optional or absent).
                        let some_other_doesnt_require = members.iter().any(|&other| {
                            if other == member {
                                return false;
                            }
                            let resolved_other = self.resolve_type_for_property_access(other);
                            let other_prop =
                                crate::query_boundaries::common::find_property_in_object(
                                    self.ctx.types,
                                    resolved_other,
                                    prop.name,
                                );
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

            if unit_match && never_match && absent_required_match {
                matching_members.push(member);
            }
        }

        // Only narrow if we eliminated at least one member.
        if matching_members.is_empty() || matching_members.len() == members.len() {
            return ctx_type;
        }

        if matching_members.len() == 1 {
            matching_members[0]
        } else {
            self.ctx
                .types
                .factory()
                .union_preserve_members(matching_members)
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
            || tsz_solver::type_queries::contains_error_type_db(self.ctx.types, property_type)
        {
            return TypeId::UNKNOWN;
        }
        if let Some(default) =
            crate::query_boundaries::common::type_parameter_default(self.ctx.types, property_type)
        {
            return default;
        }
        property_type
    }
}
