//! Infer-result conditional helpers for TS2344 constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn type_arg_evaluates_to_infer_result_conditional(
        &mut self,
        type_arg: TypeId,
    ) -> bool {
        if self.type_is_infer_result_conditional(type_arg) {
            return true;
        }

        let evaluated = self.evaluate_type_for_assignability(type_arg);
        evaluated != type_arg && self.type_is_infer_result_conditional(evaluated)
    }

    pub(super) fn infer_result_satisfies_via_check_constraint(
        &mut self,
        type_arg: TypeId,
        cond_components: (TypeId, TypeId, TypeId),
        inst_constraint: TypeId,
    ) -> bool {
        let (cond_check, cond_extends, cond_true) = cond_components;
        if self.infer_result_satisfies_via_mapped_key_subset(
            cond_check,
            cond_extends,
            cond_true,
            inst_constraint,
        ) {
            return true;
        }

        let db = self.ctx.types.as_type_database();
        let Some(check_name) = query::type_parameter_name(db, cond_check) else {
            return false;
        };
        let check_constraint = query::get_type_parameter_constraint(db, cond_check)
            .unwrap_or_else(|| query::base_constraint_of_type(db, cond_check));
        if check_constraint == TypeId::UNKNOWN
            || check_constraint == TypeId::ANY
            || check_constraint == cond_check
        {
            return false;
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        substitution.insert(check_name, check_constraint);
        let restricted = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            type_arg,
            &substitution,
        );
        let restricted = self.resolve_lazy_type(restricted);
        let restricted_evaluated = self.evaluate_type_for_assignability(restricted);
        self.is_assignable_to(restricted_evaluated, inst_constraint)
            || self.is_assignable_to(restricted, inst_constraint)
    }

    fn infer_result_satisfies_via_mapped_key_subset(
        &mut self,
        cond_check: TypeId,
        cond_extends: TypeId,
        cond_true: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        if !query::is_infer_type(db, cond_true) {
            return false;
        }
        let inst_constraint = self.resolve_lazy_type(inst_constraint);
        let Some(constraint_source) = query::keyof_operand(db, inst_constraint) else {
            return false;
        };
        let Some(check_mapped) = crate::query_boundaries::common::mapped_type_info(db, cond_check)
        else {
            return false;
        };
        let Some(extends_mapped) =
            crate::query_boundaries::common::mapped_type_info(db, cond_extends)
        else {
            return false;
        };
        if query::keyof_operand(db, check_mapped.constraint) != Some(constraint_source)
            || query::keyof_operand(db, extends_mapped.constraint) != Some(constraint_source)
        {
            return false;
        }
        if extends_mapped.template != cond_true {
            return false;
        }
        self.type_is_mapped_key_or_never(check_mapped.template, check_mapped.type_param.name)
    }

    pub(super) fn infer_result_satisfies_via_application_arg_constraints(
        &mut self,
        type_arg: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let Some((_base, args)) =
            query::application_base_and_args(self.ctx.types.as_type_database(), type_arg)
        else {
            return false;
        };

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for arg in args {
            let db = self.ctx.types.as_type_database();
            let Some(arg_name) = query::type_parameter_name(db, arg) else {
                continue;
            };
            let arg_constraint = query::get_type_parameter_constraint(db, arg)
                .unwrap_or_else(|| query::base_constraint_of_type(db, arg));
            if arg_constraint != TypeId::UNKNOWN
                && arg_constraint != TypeId::ANY
                && arg_constraint != arg
            {
                substitution.insert(arg_name, arg_constraint);
            }
        }

        if substitution.is_empty() {
            return false;
        }

        let restricted = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            type_arg,
            &substitution,
        );
        let restricted = self.resolve_lazy_type(restricted);
        let restricted_evaluated = self.evaluate_type_for_assignability(restricted);
        self.is_assignable_to(restricted_evaluated, inst_constraint)
            || self.is_assignable_to(restricted, inst_constraint)
    }

    /// Some aliases compute a constrained result through a conditional `infer`
    /// nested below a mapped or indexed access, so their application arguments
    /// are not enough to prove the result constraint directly. Recheck the
    /// original type argument after replacing referenced type parameters with
    /// their declared constraints, e.g. Redux-style `ActionFromReducers<R>`.
    pub(super) fn infer_result_satisfies_via_referenced_constraints(
        &mut self,
        type_arg: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        if !self.type_contains_infer_result_conditional(type_arg) {
            return false;
        }

        let db = self.ctx.types.as_type_database();
        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        let mut referenced =
            crate::query_boundaries::common::collect_referenced_types(db, type_arg);
        referenced.insert(type_arg);
        for ty in referenced {
            if query::is_infer_type(db, ty) {
                continue;
            }
            let Some(name) = query::type_parameter_name(db, ty) else {
                continue;
            };
            let constraint = query::get_type_parameter_constraint(db, ty)
                .unwrap_or_else(|| query::base_constraint_of_type(db, ty));
            if constraint != TypeId::UNKNOWN && constraint != TypeId::ANY && constraint != ty {
                substitution.insert(name, constraint);
            }
        }
        if substitution.is_empty() {
            return false;
        }

        let restricted = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            type_arg,
            &substitution,
        );
        let restricted = self.resolve_lazy_type(restricted);
        let restricted_evaluated = self.evaluate_type_for_assignability(restricted);
        self.is_assignable_to(restricted_evaluated, inst_constraint)
            || self.is_assignable_to(restricted, inst_constraint)
    }

    pub(super) fn infer_result_satisfies_array_like_constraint(
        &mut self,
        cond_extends: TypeId,
        cond_true: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        if !query::is_infer_type(db, cond_true)
            || !self.target_constraint_is_array_like(inst_constraint)
        {
            return false;
        }

        self.infer_type_appears_as_tuple_rest(cond_extends, cond_true)
    }

    pub(crate) fn array_element_infer_alias_satisfies_constraint(
        &mut self,
        type_arg: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let Some(candidates) = self.instantiated_alias_body_candidates(type_arg) else {
            return false;
        };

        candidates.into_iter().any(|candidate| {
            self.is_assignable_to_no_weak_checks(candidate, inst_constraint)
                || self.base_union_members_satisfy_constraint(candidate, inst_constraint)
                || self.conditional_array_element_infer_satisfies_constraint(
                    candidate,
                    inst_constraint,
                )
        })
    }

    fn instantiated_alias_body_candidates(&mut self, type_arg: TypeId) -> Option<Vec<TypeId>> {
        let db = self.ctx.types.as_type_database();
        let (base, args) = query::application_base_and_args(db, type_arg)?;
        let def_id = query::lazy_def_id(db, base)?;

        let direct_alias = self.direct_source_alias_body_for_def(def_id);
        let (body, params) = if let Some((body, params)) = direct_alias {
            (body, params)
        } else {
            let body = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_def(def_id))
                .or_else(|| self.ctx.definition_store.get_body(def_id))?;
            let params = self
                .ctx
                .get_def_type_params(def_id)
                .or_else(|| self.infer_alias_type_params_from_body(body, args.len()))?;
            (body, params)
        };
        if params.len() != args.len() {
            return None;
        }

        let instantiated = crate::query_boundaries::common::instantiate_generic(
            self.ctx.types,
            body,
            &params,
            &args,
        );

        let mut candidates = Vec::with_capacity(5);
        candidates.push(instantiated);
        candidates.push(self.resolve_lazy_type(instantiated));
        candidates.push(self.evaluate_type_with_env_uncached(instantiated));
        candidates.push(self.evaluate_type_for_assignability(instantiated));
        candidates.push(self.evaluate_type_for_assignability(type_arg));
        candidates.sort_by_key(|ty| ty.0);
        candidates.dedup();
        Some(candidates)
    }

    fn direct_source_alias_body_for_def(
        &mut self,
        def_id: tsz_solver::def::DefId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if let Some(result) = self
            .ctx
            .def_to_symbol_id_with_fallback(def_id)
            .and_then(|sym_id| self.direct_source_alias_body_for_symbol(sym_id))
        {
            return Some(result);
        }

        let name_atom = self.ctx.definition_store.get_name(def_id)?;
        let name = self.ctx.types.resolve_atom_ref(name_atom);
        let entries = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name.as_ref()))?
            .clone();
        for (file_idx, sym_id) in entries {
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                continue;
            };
            let Some(symbol) = binder.get_symbol(sym_id) else {
                continue;
            };
            if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0
                || symbol.escaped_name != name.as_ref()
            {
                continue;
            }
            if let Some(result) =
                self.direct_source_file_type_alias_result(sym_id, Some(file_idx), true)
            {
                return Some(result);
            }
        }
        None
    }

    fn direct_source_alias_body_for_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let alias_target = {
            let symbol = self.get_cross_file_symbol(sym_id)?;
            if symbol.flags & tsz_binder::symbol_flags::ALIAS == 0 {
                None
            } else {
                let module_name = symbol.import_module.clone()?;
                let import_name = symbol.import_name.clone()?;
                let source_file_idx = (symbol.decl_file_idx != u32::MAX)
                    .then_some(symbol.decl_file_idx as usize)
                    .or_else(|| self.ctx.resolve_symbol_file_index(sym_id));
                Some((module_name, import_name, source_file_idx))
            }
        };
        let target_sym_id = if let Some((module_name, import_name, source_file_idx)) = alias_target
        {
            self.resolve_cross_file_export_from_file(&module_name, &import_name, source_file_idx)?
        } else {
            sym_id
        };
        let file_idx = self.ctx.resolve_symbol_file_index(target_sym_id);
        self.direct_source_file_type_alias_result(target_sym_id, file_idx, true)
    }

    fn infer_alias_type_params_from_body(
        &self,
        body: TypeId,
        expected_len: usize,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        if expected_len != 1 {
            return None;
        }
        let db = self.ctx.types.as_type_database();
        let (check_type, _extends_type, _true_type, _false_type) =
            query::full_conditional_type_components(db, body)?;
        query::named_type_param_info(db, check_type).map(|info| vec![info])
    }

    fn conditional_array_element_infer_satisfies_constraint(
        &mut self,
        type_id: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((cond_check, cond_extends, cond_true, cond_false)) =
            query::full_conditional_type_components(db, type_id)
        else {
            return false;
        };
        if cond_false != TypeId::NEVER || !query::is_infer_type(db, cond_true) {
            return false;
        }
        if !self.array_like_pattern_extracts_infer(cond_extends, cond_true) {
            return false;
        }

        let check_candidates = [
            cond_check,
            self.resolve_lazy_type(cond_check),
            self.evaluate_type_with_env_uncached(cond_check),
            self.evaluate_type_for_assignability(cond_check),
        ];
        let elements = check_candidates
            .into_iter()
            .filter_map(|candidate| self.array_like_element_type(candidate))
            .collect::<Vec<_>>();
        elements.into_iter().any(|element| {
            self.is_assignable_to_no_weak_checks(element, inst_constraint)
                || self.base_union_members_satisfy_constraint(element, inst_constraint)
        })
    }

    fn array_like_pattern_extracts_infer(&mut self, pattern: TypeId, infer_type: TypeId) -> bool {
        let pattern_candidates = [
            pattern,
            self.resolve_lazy_type(pattern),
            self.evaluate_type_with_env_uncached(pattern),
            self.evaluate_type_for_assignability(pattern),
        ];
        pattern_candidates
            .into_iter()
            .filter_map(|candidate| self.array_like_element_type(candidate))
            .any(|element| element == infer_type)
    }

    fn array_like_element_type(&self, type_id: TypeId) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        query::array_element_type(db, type_id)
            .or_else(|| query::number_index_value_type(db, type_id))
    }

    pub(super) fn type_arg_evaluates_to_array_like_infer_result_conditional(
        &mut self,
        type_arg: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        if !self.target_constraint_is_array_like(inst_constraint) {
            return false;
        }

        let candidates = [
            type_arg,
            self.resolve_lazy_type(type_arg),
            self.evaluate_type_for_assignability(type_arg),
        ];
        candidates.into_iter().any(|candidate| {
            self.type_is_array_like_infer_result_conditional(candidate, inst_constraint)
        })
    }

    fn type_is_array_like_infer_result_conditional(
        &mut self,
        type_id: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((_cond_check, cond_extends, cond_true, cond_false)) =
            query::full_conditional_type_components(db, type_id)
        else {
            return false;
        };
        if cond_false != TypeId::NEVER {
            return false;
        }

        self.infer_result_satisfies_array_like_constraint(cond_extends, cond_true, inst_constraint)
            || self.type_is_array_like_infer_result_conditional(cond_true, inst_constraint)
    }

    fn type_is_mapped_key_or_never(&self, type_id: TypeId, key_name: tsz_common::Atom) -> bool {
        if type_id == TypeId::NEVER {
            return true;
        }

        let db = self.ctx.types.as_type_database();
        if query::type_parameter_name(db, type_id) == Some(key_name) {
            return true;
        }

        query::full_conditional_type_components(db, type_id).is_some_and(
            |(_check, _extends, true_type, false_type)| {
                self.type_is_mapped_key_or_never(true_type, key_name)
                    && self.type_is_mapped_key_or_never(false_type, key_name)
                    && (true_type != TypeId::NEVER || false_type != TypeId::NEVER)
            },
        )
    }

    fn type_is_infer_result_conditional(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        query::full_conditional_type_components(db, type_id).is_some_and(
            |(_cond_check, _cond_extends, cond_true, cond_false)| {
                cond_false == TypeId::NEVER && query::is_infer_type(db, cond_true)
            },
        )
    }

    fn type_contains_infer_result_conditional(&mut self, type_id: TypeId) -> bool {
        if self.type_or_references_include_infer_result_conditional(type_id) {
            return true;
        }

        let resolved = self.resolve_lazy_type(type_id);
        if resolved != type_id && self.type_or_references_include_infer_result_conditional(resolved)
        {
            return true;
        }

        let evaluated = self.evaluate_type_for_assignability(type_id);
        evaluated != type_id && self.type_or_references_include_infer_result_conditional(evaluated)
    }

    fn type_or_references_include_infer_result_conditional(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let mut referenced = crate::query_boundaries::common::collect_referenced_types(db, type_id);
        referenced.insert(type_id);
        referenced
            .into_iter()
            .any(|ty| self.type_is_infer_result_conditional(ty))
    }

    fn target_constraint_is_array_like(&mut self, target: TypeId) -> bool {
        let resolved = self.resolve_lazy_type(target);
        let evaluated = self.evaluate_type_for_assignability(resolved);
        let db = self.ctx.types.as_type_database();
        [target, resolved, evaluated].into_iter().any(|candidate| {
            matches!(
                query::classify_array_like(db, candidate),
                query::ArrayLikeKind::Array(_)
                    | query::ArrayLikeKind::Tuple
                    | query::ArrayLikeKind::Readonly(_)
            )
        })
    }

    fn infer_type_appears_as_tuple_rest(&mut self, pattern: TypeId, infer_type: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let candidates = [
            pattern,
            self.resolve_lazy_type(pattern),
            self.evaluate_type_for_assignability(pattern),
        ];
        candidates.into_iter().any(|candidate| {
            crate::query_boundaries::common::tuple_elements(db, candidate).is_some_and(|elements| {
                elements.iter().any(|element| {
                    element.rest
                        && (element.type_id == infer_type
                            || self.infer_type_appears_as_tuple_rest(element.type_id, infer_type))
                })
            })
        })
    }
}
