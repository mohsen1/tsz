use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeList;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn function_type_predicate_text(
        &self,
        func_type_id: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let signature =
            tsz_solver::type_queries::flow::extract_predicate_signature(interner, func_type_id)?;
        let predicate = signature.predicate;

        let mut text = String::new();
        if predicate.asserts {
            text.push_str("asserts ");
        }
        match predicate.target {
            tsz_solver::types::TypePredicateTarget::This => text.push_str("this"),
            tsz_solver::types::TypePredicateTarget::Identifier(atom) => {
                text.push_str(&interner.resolve_atom(atom));
            }
        }
        if let Some(type_id) = predicate.type_id {
            text.push_str(" is ");
            let type_text = self
                .type_parameter_strict_null_predicate_text(type_id, outer_type_params)
                .or_else(|| self.predicate_parameter_union_alias_text(&signature, type_id))
                .unwrap_or_else(|| {
                    outer_type_params
                        .filter(|type_params| !type_params.nodes.is_empty())
                        .map(|type_params| {
                            self.print_type_id_with_outer_type_params(type_id, type_params)
                        })
                        .unwrap_or_else(|| {
                            self.print_type_id_for_inferred_predicate_declaration(type_id)
                        })
                });
            text.push_str(&type_text);
        }
        Some(text)
    }

    fn predicate_parameter_union_alias_text(
        &self,
        signature: &tsz_solver::type_queries::flow::ExtractedPredicateSignature,
        predicate_type: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let param_index =
            signature
                .predicate
                .parameter_index
                .or_else(|| match signature.predicate.target {
                    tsz_solver::types::TypePredicateTarget::Identifier(target) => signature
                        .params
                        .iter()
                        .position(|param| param.name == Some(target)),
                    tsz_solver::types::TypePredicateTarget::This => None,
                })?;
        let param_type = signature.params.get(param_index)?.type_id;
        let predicate_surface = self.evaluate_type_for_predicate_alias(predicate_type);
        let param_surface = self.evaluate_type_for_predicate_alias(param_type);
        let interner = self.type_interner?;
        let union_members = tsz_solver::type_queries::get_union_members(interner, param_surface)?;

        union_members
            .iter()
            .copied()
            .filter(|member| {
                *member != tsz_solver::types::TypeId::NULL
                    && *member != tsz_solver::types::TypeId::UNDEFINED
            })
            .find_map(|member| {
                let member_surface = self.evaluate_type_for_predicate_alias(member);
                let structural_match =
                    self.predicate_alias_candidate_matches(member, predicate_type);
                (member_surface == predicate_surface || structural_match)
                    .then(|| self.predicate_alias_candidate_text(member))
                    .flatten()
            })
    }

    fn predicate_alias_candidate_text(&self, type_id: tsz_solver::types::TypeId) -> Option<String> {
        let interner = self.type_interner?;
        if self.predicate_alias_candidate_is_nameable_type(type_id, interner) {
            return Some(self.print_type_id_for_inferred_predicate_declaration(type_id));
        }

        self.nameable_alias_text_for_predicate_surface(type_id)
    }

    fn predicate_alias_candidate_is_nameable_type(
        &self,
        type_id: tsz_solver::types::TypeId,
        interner: &tsz_solver::construction::TypeInterner,
    ) -> bool {
        if type_id.is_intrinsic() || tsz_solver::visitor::literal_value(interner, type_id).is_some()
        {
            return true;
        }
        if self.should_preserve_named_type_reference_for_emit(type_id, interner) {
            return true;
        }
        let shape_id = tsz_solver::visitor::object_shape_id(interner, type_id)
            .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id));
        shape_id
            .and_then(|shape_id| interner.object_shape(shape_id).symbol)
            .is_some_and(|sym_id| self.symbol_is_nameable_type_for_emit(sym_id))
    }

    fn nameable_alias_text_for_predicate_surface(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let (Some(interner), Some(cache)) = (self.type_interner, self.type_cache.as_ref()) else {
            return None;
        };
        if type_id.is_intrinsic() || tsz_solver::visitor::literal_value(interner, type_id).is_some()
        {
            return None;
        }

        // Inferred predicate narrowing can return the structural union member
        // after alias identity is lost. Re-anchor it to a public alias body
        // before printing so declaration emit keeps the source API surface.
        let resolver = super::DtsStructuralResolver { cache };
        let mut matches: Vec<_> = cache
            .def_types
            .iter()
            .filter_map(|(&raw_def_id, &body)| {
                let def_id = tsz_solver::DefId(raw_def_id);
                if !self.predicate_alias_def_is_nameable(def_id) {
                    return None;
                }
                (body == type_id
                    || tsz_solver::computation::are_types_structurally_identical(
                        interner, &resolver, body, type_id,
                    ))
                .then_some(def_id)
            })
            .collect();
        matches.sort_by_key(|def_id| def_id.0);

        matches
            .into_iter()
            .map(|def_id| {
                self.print_type_id_for_inferred_predicate_declaration(interner.lazy(def_id))
            })
            .next()
    }

    fn predicate_alias_def_is_nameable(&self, def_id: tsz_solver::DefId) -> bool {
        let Some(cache) = self.type_cache.as_ref() else {
            return false;
        };
        cache
            .def_to_symbol
            .get(&def_id)
            .copied()
            .is_some_and(|sym_id| self.symbol_is_nameable_type_for_emit(sym_id))
            || cache.def_to_name.contains_key(&def_id)
    }

    fn predicate_alias_candidate_matches(
        &self,
        candidate: tsz_solver::types::TypeId,
        predicate_type: tsz_solver::types::TypeId,
    ) -> bool {
        let (Some(interner), Some(cache)) = (self.type_interner, &self.type_cache) else {
            return false;
        };
        let resolver = super::DtsCacheResolver { cache };
        tsz_solver::computation::are_types_structurally_identical(
            interner,
            &resolver,
            candidate,
            predicate_type,
        )
    }

    fn evaluate_type_for_predicate_alias(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> tsz_solver::types::TypeId {
        let Some(interner) = self.type_interner else {
            return type_id;
        };
        if let Some(cache) = &self.type_cache {
            let resolver = super::DtsCacheResolver { cache };
            let mut evaluator =
                tsz_solver::computation::TypeEvaluator::with_resolver(interner, &resolver);
            evaluator.set_max_mapped_keys(1_024);
            evaluator.evaluate(type_id)
        } else {
            let mut evaluator = tsz_solver::computation::TypeEvaluator::new(interner);
            evaluator.set_max_mapped_keys(1_024);
            evaluator.evaluate(type_id)
        }
    }

    fn type_parameter_strict_null_predicate_text(
        &self,
        type_id: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let is_type_param_like = |id| tsz_solver::visitor::type_param_info(interner, id).is_some();
        // Returns (type_param, other_side) so callers don't re-check which side is the param.
        let find_type_param = |a: tsz_solver::types::TypeId, b: tsz_solver::types::TypeId| {
            if is_type_param_like(a) {
                Some((a, b))
            } else if is_type_param_like(b) {
                Some((b, a))
            } else {
                None
            }
        };

        // Pattern 1: intersection T & ({} | undefined)
        if let Some(members) = tsz_solver::type_queries::get_intersection_members(interner, type_id)
            && let [left, right] = members.as_slice()
            && let Some((type_param, union_type)) = find_type_param(*left, *right)
        {
            if let Some(union_members) =
                tsz_solver::type_queries::get_union_members(interner, union_type)
                && union_members.contains(&tsz_solver::types::TypeId::UNDEFINED)
                && union_members
                    .iter()
                    .copied()
                    .any(|m| tsz_solver::type_queries::is_empty_object_type(interner, m))
            {
                return Some(
                    self.format_type_param_strict_null_predicate(type_param, outer_type_params),
                );
            }
        }

        // Pattern 2: union [(T & undefined), (T & {})] — the solver's distributive form of
        // T & ({} | undefined).  Both arms must be verified: (T & undefined) and (T & {}).
        if let Some(union_members) = tsz_solver::type_queries::get_union_members(interner, type_id)
            && union_members.len() == 2
        {
            for idx in 0..2usize {
                let candidate = union_members[idx];
                let other = union_members[1 - idx];
                if let Some(int_members) =
                    tsz_solver::type_queries::get_intersection_members(interner, candidate)
                    && let [a, b] = int_members.as_slice()
                    && (*a == tsz_solver::types::TypeId::UNDEFINED
                        || *b == tsz_solver::types::TypeId::UNDEFINED)
                    && let Some((type_param, _)) = find_type_param(*a, *b)
                    && Self::is_intersection_of_type_param_with_empty_object(
                        interner, other, type_param,
                    )
                {
                    return Some(
                        self.format_type_param_strict_null_predicate(type_param, outer_type_params),
                    );
                }
            }
        }

        None
    }

    /// Returns `true` when `type_id` is a two-member intersection `[expected_param, {}]`
    /// (in either order), confirming it is the non-nullish arm of the distributive
    /// `(T & undefined) | (T & {})` pattern.
    fn is_intersection_of_type_param_with_empty_object(
        interner: &tsz_solver::construction::TypeInterner,
        type_id: tsz_solver::types::TypeId,
        expected_param: tsz_solver::types::TypeId,
    ) -> bool {
        tsz_solver::type_queries::get_intersection_members(interner, type_id).is_some_and(
            |members| {
                let [a, b] = members.as_slice() else {
                    return false;
                };
                (*a == expected_param || *b == expected_param)
                    && (tsz_solver::type_queries::is_empty_object_type(interner, *a)
                        || tsz_solver::type_queries::is_empty_object_type(interner, *b))
            },
        )
    }

    fn format_type_param_strict_null_predicate(
        &self,
        type_param: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> String {
        let type_param_text = outer_type_params
            .filter(|tp| !tp.nodes.is_empty())
            .map(|tp| self.print_type_id_with_outer_type_params(type_param, tp))
            .unwrap_or_else(|| self.print_type_id(type_param));
        format!("{type_param_text} & ({{}} | undefined)")
    }
}
