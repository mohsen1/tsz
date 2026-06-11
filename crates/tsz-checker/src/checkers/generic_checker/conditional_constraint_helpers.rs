use crate::query_boundaries::checkers::generic as query;
use crate::query_boundaries::common::TypeResolver;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn conditional_result_branches_satisfy_constraint(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        if matches!(constraint, TypeId::ANY | TypeId::UNKNOWN)
            || query::contains_free_type_parameters(self.ctx.types, constraint)
        {
            return false;
        }
        let cache_key = (type_arg, constraint);
        if let Some(&cached) = self
            .ctx
            .type_reference_validation_caches
            .conditional_branch_constraint
            .get(&cache_key)
        {
            return cached;
        }

        // Program-wide success tier: file checkers re-prove the same
        // conditional-branch pairs for every file that references the same
        // generic alias. Probing needs no shareability gate (only gated pairs
        // are ever published); the gate runs at publish time, once per
        // distinct novel success. Only successes are shared (see
        // `SharedConstraintProofCache`).
        if let Some(shared) = &self.ctx.shared_constraint_proofs
            && shared.conditional_branch_successes.contains(&cache_key)
        {
            tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "conditional_branch", "hit");
            self.ctx
                .type_reference_validation_caches
                .conditional_branch_constraint
                .insert(cache_key, true);
            return true;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let result =
            self.conditional_result_branches_satisfy_constraint_uncached(type_arg, constraint);
        self.ctx
            .type_reference_validation_caches
            .conditional_branch_constraint
            .insert(cache_key, result);
        if result {
            self.publish_shared_constraint_proof(
                lazy_failures_at_entry,
                type_arg,
                constraint,
                |shared| {
                    tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "conditional_branch", "publish");
                    shared.conditional_branch_successes.insert(cache_key);
                },
            );
        }
        result
    }

    fn conditional_result_branches_satisfy_constraint_uncached(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let components =
            query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
                .or_else(|| self.type_alias_application_conditional_components(type_arg))
                .or_else(|| {
                    let type_arg_evaluated = self.evaluate_type_for_assignability(type_arg);
                    (type_arg_evaluated != type_arg).then(|| {
                        query::full_conditional_type_components(
                            self.ctx.types.as_type_database(),
                            type_arg_evaluated,
                        )
                    })?
                });
        let Some((check_type, extends_type, true_type, false_type)) = components else {
            return false;
        };
        let db = self.ctx.types.as_type_database();
        if [true_type, false_type]
            .into_iter()
            .any(|branch| branch != TypeId::NEVER && query::is_infer_type(db, branch))
        {
            return false;
        }

        [true_type, false_type].into_iter().all(|branch| {
            if branch == TypeId::NEVER {
                return true;
            }
            let raw_branch = branch;
            let resolved_branch = self.resolve_lazy_type(raw_branch);
            let branch_evaluated = self.evaluate_type_for_assignability(resolved_branch);
            self.indexed_object_map_branch_satisfies_constraint(raw_branch, constraint)
                || self
                    .conditional_constraint_component_relation_outcome(resolved_branch, constraint)
                    .related
                || self
                    .conditional_constraint_component_relation_outcome(branch_evaluated, constraint)
                    .related
                || self.indexed_object_map_branch_satisfies_constraint(resolved_branch, constraint)
                || (raw_branch == true_type
                    && raw_branch == check_type
                    && self.conditional_extends_type_satisfies_constraint(extends_type, constraint))
        })
    }

    fn conditional_extends_type_satisfies_constraint(
        &mut self,
        extends_type: TypeId,
        constraint: TypeId,
    ) -> bool {
        let extends_type = self.resolve_lazy_type(extends_type);
        let extends_evaluated = self.evaluate_type_for_assignability(extends_type);
        let constraint = self.resolve_lazy_type(constraint);
        let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
        self.conditional_constraint_component_relation_outcome(extends_type, constraint)
            .related
            || self
                .conditional_constraint_component_relation_outcome(
                    extends_evaluated,
                    constraint_evaluated,
                )
                .related
    }

    fn indexed_object_map_branch_satisfies_constraint(
        &mut self,
        branch: TypeId,
        constraint: TypeId,
    ) -> bool {
        let cache_key = (branch, constraint);
        if let Some(&cached) = self
            .ctx
            .type_reference_validation_caches
            .indexed_object_map_branch_constraint
            .get(&cache_key)
        {
            return cached;
        }

        // Program-wide success tier; see
        // `conditional_result_branches_satisfy_constraint` above.
        if let Some(shared) = &self.ctx.shared_constraint_proofs
            && shared
                .indexed_object_map_branch_successes
                .contains(&cache_key)
        {
            tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "indexed_object_map", "hit");
            self.ctx
                .type_reference_validation_caches
                .indexed_object_map_branch_constraint
                .insert(cache_key, true);
            return true;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let result =
            self.indexed_object_map_branch_satisfies_constraint_uncached(branch, constraint);
        self.ctx
            .type_reference_validation_caches
            .indexed_object_map_branch_constraint
            .insert(cache_key, result);
        if result {
            self.publish_shared_constraint_proof(
                lazy_failures_at_entry,
                branch,
                constraint,
                |shared| {
                    tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "indexed_object_map", "publish");
                    shared.indexed_object_map_branch_successes.insert(cache_key);
                },
            );
        }
        result
    }

    fn indexed_object_map_branch_satisfies_constraint_uncached(
        &mut self,
        branch: TypeId,
        constraint: TypeId,
    ) -> bool {
        let Some((object_type, _index_type)) =
            query::index_access_components(self.ctx.types.as_type_database(), branch)
        else {
            return false;
        };
        let object_type = self
            .resolve_alias_body_for_constraint_branch(object_type)
            .unwrap_or_else(|| self.resolve_lazy_type(object_type));
        let value_types = {
            let Some(shape) =
                query::get_object_shape(self.ctx.types.as_type_database(), object_type)
            else {
                return false;
            };
            let mut values: Vec<TypeId> = shape
                .properties
                .iter()
                .map(|property| property.type_id)
                .collect();
            if let Some(index) = &shape.string_index {
                values.push(index.value_type);
            }
            if let Some(index) = &shape.number_index {
                values.push(index.value_type);
            }
            values
        };
        if value_types.is_empty() {
            return false;
        }

        let constraint = self.resolve_lazy_type(constraint);
        let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
        value_types.into_iter().all(|value| {
            if value == TypeId::NEVER {
                return true;
            }
            let value = self.resolve_lazy_type(value);
            if query::indexed_object_map_value_structurally_satisfies_constraint(
                self.ctx.types.as_type_database(),
                value,
                constraint,
            ) || query::indexed_object_map_value_structurally_satisfies_constraint(
                self.ctx.types.as_type_database(),
                value,
                constraint_evaluated,
            ) {
                return true;
            }
            let value_evaluated = self.evaluate_type_for_assignability(value);
            self.conditional_constraint_component_relation_outcome(value, constraint)
                .related
                || self
                    .conditional_constraint_component_relation_outcome(
                        value_evaluated,
                        constraint_evaluated,
                    )
                    .related
                || self
                    .tuple_value_satisfies_tuple_constraint(value_evaluated, constraint_evaluated)
                || self.conditional_result_branches_satisfy_constraint(value, constraint)
        })
    }

    fn tuple_value_satisfies_tuple_constraint(&mut self, source: TypeId, target: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some(source_elements) = crate::query_boundaries::common::tuple_elements(db, source)
        else {
            return false;
        };
        let Some(target_elements) = crate::query_boundaries::common::tuple_elements(db, target)
        else {
            return false;
        };
        if source_elements.len() != target_elements.len() {
            return false;
        }

        source_elements.iter().zip(target_elements.iter()).all(
            |(source_element, target_element)| {
                self.conditional_constraint_component_relation_outcome(
                    source_element.type_id,
                    target_element.type_id,
                )
                .related
                    || self.literal_satisfies_keyof_constraint(
                        source_element.type_id,
                        target_element.type_id,
                    )
            },
        )
    }

    fn literal_satisfies_keyof_constraint(&mut self, source: TypeId, target: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some(name) = crate::query_boundaries::common::string_literal_value(db, source) else {
            return false;
        };
        let Some(operand) = crate::query_boundaries::common::keyof_inner_type(db, target) else {
            return false;
        };
        let operand = self
            .resolve_alias_body_for_constraint_branch(operand)
            .unwrap_or_else(|| self.resolve_lazy_type(operand));
        query::get_object_shape(self.ctx.types.as_type_database(), operand).is_some_and(|shape| {
            shape
                .properties
                .iter()
                .any(|property| property.name == name)
                || shape.string_index.is_some()
        })
    }

    fn type_alias_application_conditional_components(
        &mut self,
        mut type_arg: TypeId,
    ) -> Option<(TypeId, TypeId, TypeId, TypeId)> {
        let mut seen = FxHashSet::default();
        for _ in 0..8 {
            if !seen.insert(type_arg) {
                return None;
            }
            if let Some(components) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                let (_check_type, _extends_type, true_type, false_type) = components;
                let mut branch_is_simple = |branch| {
                    branch == TypeId::NEVER
                        || (!query::contains_free_type_parameters(self.ctx.types, branch)
                            && !query::is_infer_type(self.ctx.types.as_type_database(), branch))
                        || self.is_indexed_object_map_branch(branch)
                };
                if branch_is_simple(true_type) && branch_is_simple(false_type) {
                    return Some(components);
                }
                return None;
            }

            let app = crate::query_boundaries::common::type_application(self.ctx.types, type_arg)?;
            let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            let def = self.ctx.definition_store.get(def_id)?;
            if def.kind != tsz_solver::def::DefKind::TypeAlias
                || def.type_params.len() != app.args.len()
            {
                return None;
            }
            let body = def.body?;
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            let instantiated =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            if instantiated == type_arg {
                return None;
            }
            type_arg = instantiated;
        }
        None
    }

    fn resolve_alias_body_for_constraint_branch(&mut self, type_id: TypeId) -> Option<TypeId> {
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.unresolved_type_name_def_id(type_id))?;
        if !self
            .ctx
            .definition_store
            .get(def_id)
            .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        {
            return None;
        }
        let (sym_id, file_idx) = self.ctx.def_symbol_identity(def_id)?;
        let file_idx =
            file_idx.or_else(|| self.ctx.def_file_idx(def_id).map(|idx| idx as usize))?;
        let (body_type, _params) =
            self.direct_source_file_type_alias_result(sym_id, Some(file_idx), true)?;
        (!matches!(body_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)).then_some(body_type)
    }

    fn unresolved_type_name_def_id(&self, type_id: TypeId) -> Option<tsz_solver::DefId> {
        let name = crate::query_boundaries::spread::unresolved_type_name_atom(
            self.ctx.types.as_type_database(),
            type_id,
        )?;
        let name = self.ctx.types.resolve_atom(name);
        self.ctx
            .resolve_unresolved_type_name_from_file(&name, self.ctx.current_file_idx)
            .or_else(|| TypeResolver::resolve_unresolved_type_name(&self.ctx, &name))
    }

    fn is_indexed_object_map_branch(&mut self, branch: TypeId) -> bool {
        query::index_access_components(self.ctx.types.as_type_database(), branch)
            .and_then(|(object_type, _index_type)| {
                let object_type = self
                    .resolve_alias_body_for_constraint_branch(object_type)
                    .unwrap_or(object_type);
                query::get_object_shape(self.ctx.types.as_type_database(), object_type)
            })
            .is_some_and(|shape| {
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            })
    }

    pub(crate) fn type_alias_application_infer_result_conditional_components(
        &mut self,
        mut type_arg: TypeId,
    ) -> Option<(TypeId, TypeId, TypeId, TypeId)> {
        let mut seen = FxHashSet::default();
        for _ in 0..8 {
            if !seen.insert(type_arg) {
                return None;
            }
            if let Some(components) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                let (_check_type, _extends_type, true_type, false_type) = components;
                return (false_type == TypeId::NEVER
                    && query::is_infer_type(self.ctx.types.as_type_database(), true_type))
                .then_some(components);
            }

            let app = crate::query_boundaries::common::type_application(self.ctx.types, type_arg)?;
            let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            let def = self.ctx.definition_store.get(def_id)?;
            if def.kind != tsz_solver::def::DefKind::TypeAlias
                || def.type_params.len() != app.args.len()
            {
                return None;
            }
            let body = def.body?;
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            let instantiated =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            if instantiated == type_arg {
                return None;
            }
            type_arg = instantiated;
        }
        None
    }

    pub(crate) fn resolve_record_alias_type_for_indexed_access_value(
        &mut self,
        object_type: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, object_type)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        if self.ctx.types.resolve_atom(def.name) != "Record" {
            return None;
        }
        if def.type_params.len() != app.args.len() || def.type_params.is_empty() {
            return None;
        }
        let body = def.body?;
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &app.args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
        let evaluated = self.evaluate_type_for_assignability(instantiated);
        Some(self.resolve_lazy_type(evaluated))
    }

    pub(crate) fn type_alias_application_filters_to_constraint(
        &mut self,
        mut type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        for _ in 0..8 {
            if self.indexed_object_map_branch_satisfies_constraint(type_arg, constraint) {
                return true;
            }
            if let Some((check, extends_type, true_type, false_type)) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                if false_type != TypeId::NEVER {
                    return false;
                }
                if query::is_infer_type(self.ctx.types.as_type_database(), true_type) {
                    return false;
                }

                if true_type == check {
                    let extends_resolved = self.resolve_lazy_type(extends_type);
                    let extends_evaluated = self.evaluate_type_for_assignability(extends_resolved);
                    let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
                    return self
                        .conditional_constraint_component_relation_outcome(
                            extends_evaluated,
                            constraint_evaluated,
                        )
                        .related
                        || self
                            .conditional_constraint_component_relation_outcome(
                                extends_resolved,
                                constraint,
                            )
                            .related;
                }

                let true_resolved = self.resolve_lazy_type(true_type);
                let true_evaluated = self.evaluate_type_for_assignability(true_resolved);
                let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
                if self
                    .conditional_constraint_component_relation_outcome(
                        true_evaluated,
                        constraint_evaluated,
                    )
                    .related
                    || self
                        .conditional_constraint_component_relation_outcome(
                            true_resolved,
                            constraint,
                        )
                        .related
                    || self
                        .indexed_object_map_branch_satisfies_constraint(true_resolved, constraint)
                {
                    return true;
                }

                return false;
            }

            let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_arg)
            else {
                return false;
            };
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            else {
                return false;
            };
            let Some(def) = self.ctx.definition_store.get(def_id) else {
                return false;
            };
            if def.kind != tsz_solver::def::DefKind::TypeAlias {
                return false;
            }
            let Some(body) = def.body else {
                return false;
            };
            if def.type_params.len() != app.args.len() {
                return false;
            }
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            type_arg =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            type_arg = self.resolve_lazy_type(type_arg);
        }
        false
    }
}
