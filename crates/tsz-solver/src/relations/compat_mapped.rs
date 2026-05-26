use super::*;

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    // Extracted from `compat.rs` to keep compatibility relation logic under the file-size cap.

    /// Check if a source type is assignable to a homomorphic mapped target over itself.
    pub(super) fn is_source_assignable_to_homomorphic_mapped_target(
        &mut self,
        source: TypeId,
        target_mapped_id: crate::types::MappedTypeId,
    ) -> bool {
        let mapped = self.interner.get_mapped(target_mapped_id);

        if let Some(name_type) = mapped.name_type
            && !crate::relations::subtype::rules::generics::is_filtering_name_type(
                self.interner,
                name_type,
                &mapped,
            )
        {
            return false;
        }

        // A mapped type that removes optionality can demand properties that the
        // source may not have, so `S` is not generally assignable to `Required<S>`.
        if mapped.optional_modifier == Some(MappedModifier::Remove) {
            return false;
        }

        let Some(mapped_source) = keyof_inner_type(self.interner, mapped.constraint) else {
            return false;
        };

        if !self.homomorphic_mapped_sources_match(source, mapped_source) {
            return false;
        }

        if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
        {
            type_param_info(self.interner, template_idx).is_some_and(|idx_param| {
                idx_param.name == mapped.type_param.name && template_obj == mapped_source
            })
        } else {
            let k_type_id = self.interner.type_param(crate::types::TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(mapped.constraint),
                default: None,
                is_const: false,
            });
            let source_value_type = self.interner.index_access(mapped_source, k_type_id);
            self.configure_subtype(self.strict_function_types);
            self.subtype
                .check_subtype(source_value_type, mapped.template)
                .is_true()
        }
    }

    pub(super) fn homomorphic_mapped_sources_match(
        &self,
        source: TypeId,
        mapped_source: TypeId,
    ) -> bool {
        if source == mapped_source {
            return true;
        }

        if let (Some(source_param), Some(mapped_param)) = (
            type_param_info(self.interner, source),
            type_param_info(self.interner, mapped_source),
        ) {
            return source_param.name == mapped_param.name;
        }

        if let (Some((source_obj, source_idx)), Some((mapped_obj, mapped_idx))) = (
            index_access_parts(self.interner, source),
            index_access_parts(self.interner, mapped_source),
        ) {
            return self.homomorphic_mapped_sources_match(source_obj, mapped_obj)
                && self.homomorphic_mapped_sources_match(source_idx, mapped_idx);
        }

        false
    }

    pub(super) fn mapped_id_or_expanded_application(
        &mut self,
        type_id: TypeId,
    ) -> Option<crate::types::MappedTypeId> {
        if let Some(mapped_id) = mapped_type_id(self.interner, type_id) {
            return Some(mapped_id);
        }
        let app_id = application_id(self.interner, type_id)?;
        let expanded = self.subtype.try_expand_application(app_id)?;
        mapped_type_id(self.interner, expanded)
    }

    pub(super) fn is_homomorphic_mapped_source_assignable_to_target(
        &mut self,
        source_mapped_id: crate::types::MappedTypeId,
        target: TypeId,
    ) -> bool {
        if self
            .subtype
            .check_homomorphic_mapped_to_target(source_mapped_id, target)
        {
            return true;
        }

        let mapped = self.interner.get_mapped(source_mapped_id);

        if let Some(name_type) = mapped.name_type
            && !crate::relations::subtype::rules::generics::is_filtering_name_type(
                self.interner,
                name_type,
                &mapped,
            )
        {
            return false;
        }

        if mapped.optional_modifier == Some(MappedModifier::Add) {
            return false;
        }

        let Some(mapped_source) = keyof_inner_type(self.interner, mapped.constraint) else {
            return false;
        };

        if !self.homomorphic_mapped_sources_match(target, mapped_source) {
            return false;
        }

        let k_type_id = self.interner.type_param(mapped.type_param);
        let target_value_type = self.interner.index_access(mapped_source, k_type_id);
        self.mapped_template_structurally_assignable(mapped.template, target_value_type)
    }

    pub(super) fn mapped_template_structurally_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source == target {
            return true;
        }

        if source == TypeId::NEVER {
            return true;
        }

        if let Some(app_id) = application_id(self.interner, source)
            && let Some(expanded) = self.subtype.try_expand_application(app_id)
            && self.mapped_template_structurally_assignable(expanded, target)
        {
            return true;
        }

        if let Some(TypeData::Conditional(cond_id)) = self.interner.lookup(source) {
            let cond = self.interner.conditional_type(cond_id);
            return self.mapped_template_structurally_assignable(cond.true_type, target)
                && self.mapped_template_structurally_assignable(cond.false_type, target);
        }

        if let (Some((source_obj, source_idx)), Some((target_obj, target_idx))) = (
            index_access_parts(self.interner, source),
            index_access_parts(self.interner, target),
        ) {
            return self.homomorphic_mapped_sources_match(source_obj, target_obj)
                && self.homomorphic_mapped_sources_match(source_idx, target_idx);
        }

        if let Some(target_members_id) = union_list_id(self.interner, target) {
            return self
                .interner
                .type_list(target_members_id)
                .iter()
                .any(|member| self.mapped_template_structurally_assignable(source, *member));
        }

        if let Some(source_members_id) = crate::visitor::intersection_list_id(self.interner, source)
        {
            return self
                .interner
                .type_list(source_members_id)
                .iter()
                .any(|member| self.mapped_template_structurally_assignable(*member, target));
        }

        false
    }

    pub(super) fn union_structurally_contains_source(
        &mut self,
        target: TypeId,
        source: TypeId,
    ) -> bool {
        let Some(target_members_id) = union_list_id(self.interner, target) else {
            return false;
        };
        let target_members: Vec<TypeId> = self.interner.type_list(target_members_id).to_vec();

        if let Some(source_members_id) = union_list_id(self.interner, source) {
            let source_members: Vec<TypeId> = self.interner.type_list(source_members_id).to_vec();
            if source_members.iter().all(|source_member| {
                target_members.iter().any(|target_member| {
                    self.structurally_same_recursive_member(*source_member, *target_member, 8)
                })
            }) {
                return true;
            }
            return self.union_has_same_arm_kinds_plus_nullish(&source_members, &target_members);
        }

        target_members
            .iter()
            .any(|target_member| self.structurally_same_recursive_member(source, *target_member, 8))
    }

    pub(super) fn union_has_same_arm_kinds_plus_nullish(
        &mut self,
        source_members: &[TypeId],
        target_members: &[TypeId],
    ) -> bool {
        if target_members.len() != source_members.len() + 1 {
            return false;
        }
        let nullish_count = target_members
            .iter()
            .filter(|member| matches!(**member, TypeId::NULL | TypeId::UNDEFINED))
            .count();
        if nullish_count != 1 {
            return false;
        }
        source_members.iter().all(|source_member| {
            target_members.iter().any(|target_member| {
                !matches!(*target_member, TypeId::NULL | TypeId::UNDEFINED)
                    && self.same_top_level_relation_shape(*source_member, *target_member)
                    && self.is_assignable(*source_member, *target_member)
            })
        })
    }

    pub(super) fn same_top_level_relation_shape(&self, left: TypeId, right: TypeId) -> bool {
        matches!(
            (self.interner.lookup(left), self.interner.lookup(right)),
            (Some(TypeData::Tuple(_)), Some(TypeData::Tuple(_)))
                | (
                    Some(TypeData::Conditional(_)),
                    Some(TypeData::Conditional(_))
                )
                | (Some(TypeData::Mapped(_)), Some(TypeData::Mapped(_)))
                | (Some(TypeData::Object(_)), Some(TypeData::Object(_)))
                | (
                    Some(TypeData::Application(_)),
                    Some(TypeData::Application(_))
                )
                | (Some(TypeData::Lazy(_)), Some(TypeData::Lazy(_)))
        )
    }

    pub(super) fn structurally_same_recursive_member(
        &self,
        left: TypeId,
        right: TypeId,
        depth: u8,
    ) -> bool {
        if left == right {
            return true;
        }
        if depth == 0 {
            return true;
        }
        if left.is_intrinsic() || right.is_intrinsic() {
            return false;
        }

        if let (Some(left_param), Some(right_param)) = (
            type_param_info(self.interner, left),
            type_param_info(self.interner, right),
        ) {
            return left_param.name == right_param.name;
        }

        if let (Some((left_obj, left_idx)), Some((right_obj, right_idx))) = (
            index_access_parts(self.interner, left),
            index_access_parts(self.interner, right),
        ) {
            return self.structurally_same_recursive_member(left_obj, right_obj, depth - 1)
                && self.structurally_same_recursive_member(left_idx, right_idx, depth - 1);
        }

        if let (Some(left_tuple), Some(right_tuple)) = (
            tuple_list_id(self.interner, left),
            tuple_list_id(self.interner, right),
        ) {
            let left_elems = self.interner.tuple_list(left_tuple);
            let right_elems = self.interner.tuple_list(right_tuple);
            return left_elems.len() == right_elems.len()
                && left_elems
                    .iter()
                    .zip(right_elems.iter())
                    .all(|(left, right)| {
                        left.optional == right.optional
                            && left.rest == right.rest
                            && self.structurally_same_recursive_member(
                                left.type_id,
                                right.type_id,
                                depth - 1,
                            )
                    });
        }

        if let (Some(left_elem), Some(right_elem)) = (
            array_element_type(self.interner, left),
            array_element_type(self.interner, right),
        ) {
            return self.structurally_same_recursive_member(left_elem, right_elem, depth - 1);
        }

        match (self.interner.lookup(left), self.interner.lookup(right)) {
            (Some(TypeData::Conditional(left_id)), Some(TypeData::Conditional(right_id))) => {
                let left_cond = self.interner.conditional_type(left_id);
                let right_cond = self.interner.conditional_type(right_id);
                left_cond.is_distributive == right_cond.is_distributive
                    && self.structurally_same_recursive_member(
                        left_cond.check_type,
                        right_cond.check_type,
                        depth - 1,
                    )
                    && self.structurally_same_recursive_member(
                        left_cond.extends_type,
                        right_cond.extends_type,
                        depth - 1,
                    )
                    && self.structurally_same_recursive_member(
                        left_cond.true_type,
                        right_cond.true_type,
                        depth - 1,
                    )
                    && self.structurally_same_recursive_member(
                        left_cond.false_type,
                        right_cond.false_type,
                        depth - 1,
                    )
            }
            (Some(TypeData::Application(left_id)), Some(TypeData::Application(right_id))) => {
                let left_app = self.interner.type_application(left_id);
                let right_app = self.interner.type_application(right_id);
                self.structurally_same_recursive_member(left_app.base, right_app.base, depth - 1)
                    && left_app.args.len() == right_app.args.len()
                    && left_app.args.iter().zip(right_app.args.iter()).all(
                        |(left_arg, right_arg)| {
                            self.structurally_same_recursive_member(
                                *left_arg,
                                *right_arg,
                                depth - 1,
                            )
                        },
                    )
            }
            (Some(TypeData::Union(left_id)), Some(TypeData::Union(right_id))) => {
                let left_members = self.interner.type_list(left_id);
                let right_members = self.interner.type_list(right_id);
                left_members.len() == right_members.len()
                    && left_members.iter().all(|left_member| {
                        right_members.iter().any(|right_member| {
                            self.structurally_same_recursive_member(
                                *left_member,
                                *right_member,
                                depth - 1,
                            )
                        })
                    })
            }
            (Some(TypeData::Mapped(left_id)), Some(TypeData::Mapped(right_id))) => {
                let left_mapped = self.interner.mapped_type(left_id);
                let right_mapped = self.interner.mapped_type(right_id);
                left_mapped.readonly_modifier == right_mapped.readonly_modifier
                    && left_mapped.optional_modifier == right_mapped.optional_modifier
                    && self.structurally_same_recursive_member(
                        left_mapped.constraint,
                        right_mapped.constraint,
                        depth - 1,
                    )
                    && match (left_mapped.name_type, right_mapped.name_type) {
                        (Some(left_name), Some(right_name)) => self
                            .structurally_same_recursive_member(left_name, right_name, depth - 1),
                        (None, None) => true,
                        _ => false,
                    }
                    && self.structurally_same_recursive_member(
                        left_mapped.template,
                        right_mapped.template,
                        depth - 1,
                    )
            }
            (Some(TypeData::Object(left_id)), Some(TypeData::Object(right_id))) => {
                let left_shape = self.interner.object_shape(left_id);
                let right_shape = self.interner.object_shape(right_id);
                left_shape.properties.len() == right_shape.properties.len()
                    && left_shape.string_index.is_some() == right_shape.string_index.is_some()
                    && left_shape.number_index.is_some() == right_shape.number_index.is_some()
                    && left_shape
                        .properties
                        .iter()
                        .zip(right_shape.properties.iter())
                        .all(|(left_prop, right_prop)| {
                            left_prop.name == right_prop.name
                                && left_prop.optional == right_prop.optional
                                && left_prop.readonly == right_prop.readonly
                                && self.structurally_same_recursive_member(
                                    left_prop.type_id,
                                    right_prop.type_id,
                                    depth - 1,
                                )
                        })
            }
            (Some(TypeData::Lazy(left_def)), Some(TypeData::Lazy(right_def))) => {
                left_def == right_def
            }
            _ => false,
        }
    }

    /// Check if two mapped types are assignable via structural template comparison.
    ///
    /// When both source and target are mapped types with the same constraint
    /// (e.g., both iterate over `keyof T`), compare their templates directly.
    /// This handles cases like `Readonly<T>` assignable to `Partial<T>` where
    /// the mapped types can't be concretely expanded because T is generic.
    ///
    /// Returns `Some(true/false)` if determination was made, `None` to fall through.
    pub(super) fn check_mapped_to_mapped_assignability(
        &mut self,
        s_mapped_id: crate::types::MappedTypeId,
        t_mapped_id: crate::types::MappedTypeId,
    ) -> Option<bool> {
        use crate::relations::subtype::rules::generics::flatten_mapped_chain;
        use crate::types::MappedModifier;
        use crate::visitor::mapped_type_id;

        // Fast path: flatten nested homomorphic chains (e.g. Partial<Readonly<T>>).
        // `flatten_mapped_chain` returns None for any mapped type that has a
        // name_type (`as` clause), so name-type compatibility is implicit here.
        if let (Some(s_flat), Some(t_flat)) = (
            flatten_mapped_chain(self.interner, s_mapped_id),
            flatten_mapped_chain(self.interner, t_mapped_id),
        ) {
            let constraints_match =
                self.mapped_key_constraint_covers(s_flat.key_constraint, t_flat.key_constraint);
            let sources_match = if s_flat.source == t_flat.source {
                true
            } else {
                self.configure_subtype(self.strict_function_types);
                self.subtype.is_subtype_of(s_flat.source, t_flat.source)
            };

            if constraints_match && sources_match {
                // Source has optional but target doesn't → reject
                if s_flat.has_optional && !t_flat.has_optional {
                    return Some(false);
                }
                return Some(true);
            }
        }

        // Fallback: single-level mapped type comparison
        let s_mapped = self.interner.get_mapped(s_mapped_id);
        let t_mapped = self.interner.get_mapped(t_mapped_id);

        // Name-type compatibility is always required: a source with no `as`
        // clause cannot be compatible with a target that renames its keys (and
        // vice-versa), regardless of how the raw key constraints relate.
        let name_types_ok = self.mapped_name_types_compatible(&s_mapped, &t_mapped);

        // Both must have the same constraint (e.g., both `keyof T`).
        // First try identity, then evaluate to normalize (e.g., keyof(Readonly<T>) → keyof(T)).
        let constraints_match = name_types_ok
            && (self.mapped_key_constraint_covers(s_mapped.constraint, t_mapped.constraint)
                || self
                    .subtype
                    .is_subtype_of(t_mapped.constraint, s_mapped.constraint));

        if !constraints_match {
            return None;
        }

        let source_template = s_mapped.template;
        let mut target_template = t_mapped.template;
        let source_param = self.interner.type_param(s_mapped.type_param);
        let target_key_substitution =
            TypeSubstitution::single(t_mapped.type_param.name, source_param);
        target_template = instantiate_type_cached(
            self.interner,
            self.query_db,
            target_template,
            &target_key_substitution,
        );

        // If the target adds optional (`?`), the target template effectively
        // becomes `template | undefined` since optional properties accept undefined.
        let target_adds_optional = t_mapped.optional_modifier == Some(MappedModifier::Add);
        let source_adds_optional = s_mapped.optional_modifier == Some(MappedModifier::Add);

        if target_adds_optional && !source_adds_optional {
            target_template = self.interner.union2(target_template, TypeId::UNDEFINED);
        }

        let target_param = self.interner.type_param(t_mapped.type_param);
        let equiv_start = self.subtype.type_param_equivalences.len();
        self.subtype
            .type_param_equivalences
            .push((source_param, target_param));

        let structurally_assignable =
            self.mapped_template_structurally_assignable(source_template, target_template);
        if structurally_assignable {
            self.subtype.type_param_equivalences.truncate(equiv_start);
            return Some(true);
        }

        // If the target removes optional (Required) but source doesn't,
        // fall through to full structural check.
        let target_removes_optional = t_mapped.optional_modifier == Some(MappedModifier::Remove);
        if target_removes_optional && !source_adds_optional && s_mapped.optional_modifier.is_none()
        {
            self.subtype.type_param_equivalences.truncate(equiv_start);
            return None;
        }

        // If both templates are themselves mapped types, recurse
        if let (Some(s_inner), Some(t_inner)) = (
            mapped_type_id(self.interner, source_template),
            mapped_type_id(self.interner, target_template),
        ) {
            self.subtype.type_param_equivalences.truncate(equiv_start);
            return self.check_mapped_to_mapped_assignability(s_inner, t_inner);
        }

        // Compare templates using the subtype checker
        self.configure_subtype(self.strict_function_types);
        let result = self.subtype.is_subtype_of(source_template, target_template);
        self.subtype.type_param_equivalences.truncate(equiv_start);
        Some(result)
    }

    /// Returns whether the `as` clause is the identity — the bare iteration
    /// variable itself (`as K`). Such clauses are structurally equivalent to
    /// having no `as` clause at all.
    pub(super) fn is_identity_name_type(&self, mapped: &MappedType) -> bool {
        let Some(name) = mapped.name_type else {
            return true;
        };
        type_param_info(self.interner, name).is_some_and(|p| p.name == mapped.type_param.name)
    }

    pub(super) fn mapped_name_types_compatible(
        &mut self,
        source_mapped: &MappedType,
        target_mapped: &MappedType,
    ) -> bool {
        // Normalize: `as K` where K is the bare iteration variable is semantically
        // equivalent to no `as` clause. Treat identity clauses as None.
        let source_name = if self.is_identity_name_type(source_mapped) {
            None
        } else {
            source_mapped.name_type
        };
        let target_name_type = if self.is_identity_name_type(target_mapped) {
            None
        } else {
            target_mapped.name_type
        };

        let (Some(source_name), Some(target_name)) = (source_name, target_name_type) else {
            return source_name == target_name_type;
        };

        let source_param = self.interner.type_param(source_mapped.type_param);
        let target_param = self.interner.type_param(target_mapped.type_param);
        let equiv_start = self.subtype.type_param_equivalences.len();
        self.subtype
            .type_param_equivalences
            .push((source_param, target_param));
        let compatible = self.subtype.is_subtype_of(source_name, target_name)
            && self.subtype.is_subtype_of(target_name, source_name);
        self.subtype.type_param_equivalences.truncate(equiv_start);
        compatible
    }

    pub(super) fn mapped_key_constraint_covers(
        &mut self,
        source_constraint: TypeId,
        target_constraint: TypeId,
    ) -> bool {
        if source_constraint == target_constraint {
            return true;
        }
        let source_eval = self.subtype.evaluate_type(source_constraint);
        let target_eval = self.subtype.evaluate_type(target_constraint);
        if source_eval != source_constraint || target_eval != target_constraint {
            return self.mapped_key_constraint_covers(source_eval, target_eval);
        }
        if let Some(target_param) = type_param_info(self.interner, target_constraint)
            && let Some(target_bound) = target_param.constraint
        {
            return self.mapped_key_constraint_covers(source_constraint, target_bound);
        }
        if type_param_info(self.interner, source_constraint).is_some() {
            return false;
        }
        if let (Some(source_obj), Some(target_obj)) = (
            keyof_inner_type(self.interner, source_constraint),
            keyof_inner_type(self.interner, target_constraint),
        ) {
            self.configure_subtype(self.strict_function_types);
            return self.subtype.is_subtype_of(source_obj, target_obj);
        }
        self.configure_subtype(self.strict_function_types);
        self.subtype
            .is_subtype_of(target_constraint, source_constraint)
    }
}
