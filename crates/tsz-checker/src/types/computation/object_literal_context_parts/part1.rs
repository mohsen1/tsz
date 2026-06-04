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
                _ => self.ctx.types.factory().intersection(filtered),
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
            _ => Some(self.ctx.types.factory().union_preserve_members(candidates)),
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
    #[allow(dead_code)] // Reserved for contextual typing improvements
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
                return Some(
                    self.ctx
                        .types
                        .factory()
                        .union_preserve_members(callable_members),
                );
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
                _ => Some(self.ctx.types.factory().intersection(callable_members)),
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
                let had_prior_complexity = self.ctx.types.take_union_too_complex();
                let evaluated = self.evaluate_type_with_env(member);
                let evaluated = self.resolve_lazy_type(evaluated);
                let evaluated = self.evaluate_application_type(evaluated);
                let produced_complexity = self.ctx.types.take_union_too_complex();
                if had_prior_complexity || produced_complexity {
                    self.ctx.types.mark_union_too_complex();
                }
                if produced_complexity {
                    return None;
                }
                common::is_callable_type(self.ctx.types, evaluated).then_some(evaluated)
            })
            .collect();

        match callable_members.len() {
            0 => None,
            1 => Some(callable_members[0]),
            _ => Some(
                self.ctx
                    .types
                    .factory()
                    .union_preserve_members(callable_members),
            ),
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
            return Some(self.sanitize_contextual_property_type(property_type));
        }

        // If contextual extraction fails but the parent context is generic/deferred,
        // preserve an `unknown` contextual slot to prevent false implicit-any
        // diagnostics during higher-order inference rounds.
        if common::contains_type_parameters(self.ctx.types, contextual_type)
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
            self.ctx.types.literal_number(value)
        } else {
            common::create_string_literal_type(self.ctx.types, property_name)
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

        if common::union_members(self.ctx.types, current)
            .is_some_and(|members| members.contains(&candidate))
        {
            return Some(candidate);
        }
        if common::union_members(self.ctx.types, candidate)
            .is_some_and(|members| members.contains(&current))
        {
            return Some(current);
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
}
