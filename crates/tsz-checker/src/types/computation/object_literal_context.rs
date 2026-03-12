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
                if let Some(shape) =
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                {
                    for prop in &shape.properties {
                        if crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            prop.type_id,
                        )
                        .is_some()
                        {
                            candidates.push(prop.type_id);
                        }
                    }

                    if let Some(index) = &shape.string_index
                        && crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            index.value_type,
                        )
                        .is_some()
                    {
                        candidates.push(index.value_type);
                    }

                    if let Some(index) = &shape.number_index
                        && crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            index.value_type,
                        )
                        .is_some()
                    {
                        candidates.push(index.value_type);
                    }
                }
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

        if tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, type_id)
            || tsz_solver::type_queries::get_mapped_type(self.ctx.types, type_id).is_some()
            || tsz_solver::type_queries::get_type_application(self.ctx.types, type_id).is_some()
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
        let evaluated_type = self.resolve_type_for_property_access(evaluated_type);
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
            ExcessPropertiesKind::NotObject => true,
        }
    }

    fn precise_callable_context_type(&mut self, type_id: TypeId) -> Option<TypeId> {
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

    fn contextual_property_presence(
        &mut self,
        type_id: TypeId,
        property_name: &str,
        depth: usize,
    ) -> ContextualPropertyPresence {
        use crate::query_boundaries::assignability::ExcessPropertiesKind;
        use tsz_solver::operations::property::PropertyAccessResult;

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

        let prev_context = self.ctx.contextual_type;
        if target_type != TypeId::ANY
            && target_type != TypeId::NEVER
            && target_type != TypeId::UNKNOWN
            && !self.type_contains_error(target_type)
        {
            self.ctx.contextual_type =
                self.contextual_type_option_for_expression(Some(target_type));
        }
        let default_type = self.get_type_of_node(default_idx);
        self.ctx.contextual_type = prev_context;

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
                    use tsz_solver::TypeEvaluator;

                    let mut evaluator = TypeEvaluator::with_resolver(this.ctx.types, &this.ctx);
                    let alternate_member = evaluator.evaluate(member);
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
            use tsz_solver::TypeEvaluator;

            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &self.ctx);
            evaluator.evaluate(original_contextual_type)
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
        if property_presence == ContextualPropertyPresence::Absent
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
            && property_presence != ContextualPropertyPresence::Absent
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
