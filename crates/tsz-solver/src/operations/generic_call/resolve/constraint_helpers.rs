//! Constraint and callback-arity helpers for generic call resolution.

use super::*;

/// Bound resolver-backed alias materialization during dependent constraint
/// validation. This path only runs after the ordinary relation rejects the
/// inferred candidate, so keeping it request-local and bounded avoids turning
/// deeply recursive aliases into unbounded generic-call work.
const MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS: usize = 64;

#[inline]
fn enter_raw_constraint_materialization(
    constraint: TypeId,
    active: &mut FxHashSet<TypeId>,
    steps: &mut usize,
) -> bool {
    if *steps >= MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS || !active.insert(constraint) {
        return false;
    }
    *steps += 1;
    true
}

#[cfg(test)]
mod raw_constraint_materialization_guard_tests {
    use super::*;

    #[test]
    fn fuel_stops_before_the_sixty_fifth_unique_node() {
        let mut active = FxHashSet::default();
        let mut steps = 0;
        for offset in 0..MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS {
            assert!(enter_raw_constraint_materialization(
                TypeId(1_000 + offset as u32),
                &mut active,
                &mut steps,
            ));
        }
        assert!(!enter_raw_constraint_materialization(
            TypeId(2_000),
            &mut active,
            &mut steps,
        ));
        assert_eq!(steps, MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS);
    }

    #[test]
    fn cycle_guard_is_path_scoped_for_shared_nodes() {
        let shared = TypeId(1_000);
        let mut active = FxHashSet::default();
        let mut steps = 0;
        assert!(enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        assert!(!enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        active.remove(&shared);
        assert!(enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        assert_eq!(steps, 2);
    }
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn object_constraint_properties_are_any(&self, constraint: TypeId) -> bool {
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.interner.lookup(constraint)
        else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        !shape.properties.is_empty()
            && shape
                .properties
                .iter()
                .all(|prop| prop.type_id == TypeId::ANY && prop.write_type == TypeId::ANY)
    }

    fn raw_instantiated_constraint_may_satisfy(&self, constraint: TypeId) -> bool {
        if matches!(
            self.interner.lookup(constraint),
            Some(
                TypeData::Application(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
                    | TypeData::Mapped(_)
                    | TypeData::StringIntrinsic { .. }
            )
        ) {
            return true;
        }
        let mut visited = FxHashSet::default();
        let mut steps = 0;
        self.raw_instantiated_constraint_may_satisfy_inner(constraint, &mut visited, &mut steps)
    }

    fn raw_instantiated_constraint_may_satisfy_inner(
        &self,
        constraint: TypeId,
        visited: &mut FxHashSet<TypeId>,
        steps: &mut usize,
    ) -> bool {
        if *steps >= MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS || !visited.insert(constraint) {
            return false;
        }
        *steps += 1;
        match self.interner.lookup(constraint) {
            Some(
                TypeData::Application(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::Mapped(_)
                | TypeData::StringIntrinsic { .. },
            ) => true,
            Some(TypeData::Conditional(id)) => {
                let conditional = self.interner.get_conditional(id);
                [
                    conditional.check_type,
                    conditional.extends_type,
                    conditional.true_type,
                    conditional.false_type,
                ]
                .into_iter()
                .any(|part| {
                    self.raw_instantiated_constraint_may_satisfy_inner(part, visited, steps)
                })
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                self.interner.type_list(members).iter().any(|&member| {
                    self.raw_instantiated_constraint_may_satisfy_inner(member, visited, steps)
                })
            }
            Some(
                TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner),
            ) => self.raw_instantiated_constraint_may_satisfy_inner(inner, visited, steps),
            _ => false,
        }
    }

    fn raw_instantiated_constraint_root_may_need_materialization(
        &self,
        constraint: TypeId,
    ) -> bool {
        matches!(
            self.interner.lookup(constraint),
            Some(
                TypeData::Application(_)
                    | TypeData::Conditional(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
                    | TypeData::Mapped(_)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::Union(_)
                    | TypeData::Intersection(_)
                    | TypeData::Array(_)
                    | TypeData::ReadonlyType(_)
                    | TypeData::NoInfer(_)
            )
        )
    }

    pub(super) fn satisfies_raw_instantiated_constraint(
        &mut self,
        source: TypeId,
        constraint: TypeId,
        already_checked_constraint: TypeId,
    ) -> bool {
        if !self.raw_instantiated_constraint_root_may_need_materialization(constraint)
            || !self.raw_instantiated_constraint_may_satisfy(constraint)
        {
            return false;
        }
        if constraint != already_checked_constraint
            && self.checker.is_assignable_to(source, constraint)
        {
            return true;
        }

        // Imported aliases can remain as a chain of applications after the
        // instantiated constraint is evaluated. A single expansion is not
        // enough for shapes such as `ValueOrList<FieldOutput<...>>`, where
        // the conditional's check type, a union arm, and its array element are
        // aliases declared in other modules. Expand applications before asking
        // the ordinary evaluator to reduce their containing meta-types; doing
        // that in the opposite order can commit a still-opaque conditional to
        // `never`.
        let mut memo = FxHashMap::default();
        let mut active = FxHashSet::default();
        let mut steps = 0;
        let materialized = self.materialize_raw_instantiated_constraint(
            constraint,
            &mut memo,
            &mut active,
            &mut steps,
        );
        if materialized == TypeId::ERROR || materialized == constraint {
            return false;
        }
        if self.checker.is_assignable_to(source, materialized) {
            return true;
        }

        // Relations over a freshly rebuilt readonly-array/union surface can
        // still retain opaque provenance on a nested member. Preserve the
        // ordinary union and container rules while comparing their already
        // materialized leaves. This is also what lets an un-widened array
        // literal element union satisfy a dependent readonly-list arm.
        let mut relation_memo = FxHashMap::default();
        let mut relation_active = FxHashSet::default();
        let mut relation_steps = 0;
        self.source_satisfies_materialized_container(
            source,
            materialized,
            &mut relation_memo,
            &mut relation_active,
            &mut relation_steps,
        )
    }

    fn source_satisfies_materialized_container(
        &mut self,
        source: TypeId,
        target: TypeId,
        memo: &mut FxHashMap<(TypeId, TypeId), bool>,
        active: &mut FxHashSet<(TypeId, TypeId)>,
        steps: &mut usize,
    ) -> bool {
        let pair = (source, target);
        if let Some(&satisfied) = memo.get(&pair) {
            return satisfied;
        }
        if *steps >= MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS || !active.insert(pair) {
            return false;
        }
        *steps += 1;
        let satisfied = if self.checker.is_assignable_to(source, target) {
            true
        } else if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members).to_vec();
            !members.is_empty()
                && members.into_iter().all(|member| {
                    self.source_satisfies_materialized_container(
                        member, target, memo, active, steps,
                    )
                })
        } else {
            match self.interner.lookup(target) {
                Some(TypeData::Union(members)) => self
                    .interner
                    .type_list(members)
                    .to_vec()
                    .into_iter()
                    .any(|member| {
                        self.source_satisfies_materialized_container(
                            source, member, memo, active, steps,
                        )
                    }),
                Some(TypeData::Intersection(members)) => {
                    let members = self.interner.type_list(members).to_vec();
                    !members.is_empty()
                        && members.into_iter().all(|member| {
                            self.source_satisfies_materialized_container(
                                source, member, memo, active, steps,
                            )
                        })
                }
                Some(TypeData::NoInfer(inner)) => {
                    self.source_satisfies_materialized_container(source, inner, memo, active, steps)
                }
                Some(TypeData::ReadonlyType(_)) => {
                    let db = self.interner.as_type_database();
                    let Some(source_element) =
                        crate::type_queries::get_array_element_type(db, source)
                    else {
                        active.remove(&pair);
                        memo.insert(pair, false);
                        return false;
                    };
                    let Some(target_element) =
                        crate::type_queries::get_array_element_type(db, target)
                    else {
                        active.remove(&pair);
                        memo.insert(pair, false);
                        return false;
                    };
                    self.source_satisfies_materialized_container(
                        source_element,
                        target_element,
                        memo,
                        active,
                        steps,
                    )
                }
                Some(TypeData::Array(target_element)) => {
                    let Some(TypeData::Array(source_element)) = self.interner.lookup(source) else {
                        active.remove(&pair);
                        memo.insert(pair, false);
                        return false;
                    };
                    self.source_satisfies_materialized_container(
                        source_element,
                        target_element,
                        memo,
                        active,
                        steps,
                    )
                }
                _ => false,
            }
        };
        active.remove(&pair);
        memo.insert(pair, satisfied);
        satisfied
    }

    /// Return true only when static template text proves that a concrete string
    /// literal cannot match. Interpolated spans are treated as arbitrary-width
    /// wildcards, so this cannot reject a possible match.
    fn string_literal_definitely_misses_template(
        &self,
        literal_type: TypeId,
        template_type: TypeId,
    ) -> bool {
        let Some(literal_atom) =
            crate::visitors::visitor_extract::literal_string(self.interner, literal_type)
        else {
            return false;
        };
        let Some(TypeData::TemplateLiteral(spans_id)) = self.interner.lookup(template_type) else {
            return false;
        };
        let literal = self.interner.resolve_atom_ref(literal_atom);
        let spans = self.interner.template_list(spans_id);
        let mut remaining = literal.as_ref();

        for span in spans.iter() {
            let crate::types::TemplateSpan::Text(text_atom) = span else {
                continue;
            };
            let text = self.interner.resolve_atom_ref(*text_atom);
            if text.is_empty() {
                continue;
            }
            let Some(offset) = remaining.find(text.as_ref()) else {
                return true;
            };
            remaining = &remaining[offset + text.len()..];
        }
        false
    }

    fn materialize_raw_instantiated_constraint(
        &mut self,
        constraint: TypeId,
        memo: &mut FxHashMap<TypeId, TypeId>,
        active: &mut FxHashSet<TypeId>,
        steps: &mut usize,
    ) -> TypeId {
        if constraint.is_intrinsic() {
            return constraint;
        }
        if let Some(&materialized) = memo.get(&constraint) {
            return materialized;
        }
        if !enter_raw_constraint_materialization(constraint, active, steps) {
            return constraint;
        }

        let materialized = if let Some(expanded) =
            self.checker.expand_type_alias_application(constraint)
            && expanded != constraint
            && expanded != TypeId::ERROR
        {
            self.materialize_raw_instantiated_constraint(expanded, memo, active, steps)
        } else {
            match self.interner.lookup(constraint) {
                Some(TypeData::Union(members)) => {
                    let members = self.interner.type_list(members).to_vec();
                    let materialized = members
                        .iter()
                        .map(|&member| {
                            self.materialize_raw_instantiated_constraint(
                                member, memo, active, steps,
                            )
                        })
                        .collect::<Vec<_>>();
                    if materialized == members {
                        constraint
                    } else {
                        self.interner.union(materialized)
                    }
                }
                Some(TypeData::Intersection(members)) => {
                    let members = self.interner.type_list(members).to_vec();
                    let materialized = members
                        .iter()
                        .map(|&member| {
                            self.materialize_raw_instantiated_constraint(
                                member, memo, active, steps,
                            )
                        })
                        .collect::<Vec<_>>();
                    if materialized == members {
                        constraint
                    } else {
                        self.interner.intersection(materialized)
                    }
                }
                Some(TypeData::NoInfer(inner)) => {
                    let materialized =
                        self.materialize_raw_instantiated_constraint(inner, memo, active, steps);
                    if materialized == inner {
                        constraint
                    } else {
                        self.interner.no_infer(materialized)
                    }
                }
                Some(TypeData::Array(inner)) => {
                    let materialized =
                        self.materialize_raw_instantiated_constraint(inner, memo, active, steps);
                    if materialized == inner {
                        constraint
                    } else {
                        self.interner.array(materialized)
                    }
                }
                Some(TypeData::ReadonlyType(inner)) => {
                    let materialized =
                        self.materialize_raw_instantiated_constraint(inner, memo, active, steps);
                    if materialized == inner {
                        constraint
                    } else {
                        self.interner.readonly_type(materialized)
                    }
                }
                Some(TypeData::Conditional(id)) => {
                    let conditional = self.interner.get_conditional(id);
                    let check_type = self.materialize_raw_instantiated_constraint(
                        conditional.check_type,
                        memo,
                        active,
                        steps,
                    );
                    let extends_type = self.materialize_raw_instantiated_constraint(
                        conditional.extends_type,
                        memo,
                        active,
                        steps,
                    );
                    let rebuilt = if check_type == conditional.check_type
                        && extends_type == conditional.extends_type
                    {
                        constraint
                    } else {
                        self.interner.conditional(crate::types::ConditionalType {
                            check_type,
                            extends_type,
                            ..conditional
                        })
                    };
                    let concrete_scalar_check =
                        crate::visitors::visitor_predicates::is_primitive_type(
                            self.interner.as_type_database(),
                            check_type,
                        ) && !matches!(
                            check_type,
                            TypeId::ANY | TypeId::UNKNOWN | TypeId::NEVER | TypeId::ERROR
                        ) && !crate::visitors::visitor_predicates::contains_infer_types(
                            self.interner.as_type_database(),
                            extends_type,
                        ) && !crate::type_queries::contains_type_parameters_db(
                            self.interner.as_type_database(),
                            extends_type,
                        );
                    if concrete_scalar_check {
                        let branch = if self.checker.is_assignable_to(check_type, extends_type) {
                            conditional.true_type
                        } else {
                            conditional.false_type
                        };
                        self.materialize_raw_instantiated_constraint(branch, memo, active, steps)
                    } else {
                        let evaluated = self.checker.evaluate_type(rebuilt);
                        // The conditional evaluator can conservatively yield
                        // `never` for a concrete literal checked against a
                        // template containing `infer`. Select the false branch
                        // only when the template's static text proves a
                        // mismatch; a template such as `${infer T}` remains
                        // undecided because its true branch may legitimately
                        // evaluate to `never`.
                        let literal_misses_template = self
                            .string_literal_definitely_misses_template(check_type, extends_type);
                        let check_is_primitive_compound_for_object =
                            crate::type_queries::is_literal_or_primitive_or_compound_of_those(
                                self.interner.as_type_database(),
                                check_type,
                            ) && crate::visitors::visitor_predicates::is_object_like_type(
                                self.interner.as_type_database(),
                                extends_type,
                            );
                        let concrete_primitive_false_branch = evaluated == TypeId::NEVER
                            && (literal_misses_template || check_is_primitive_compound_for_object)
                            && !self.checker.is_assignable_to(check_type, extends_type);
                        if concrete_primitive_false_branch {
                            self.materialize_raw_instantiated_constraint(
                                conditional.false_type,
                                memo,
                                active,
                                steps,
                            )
                        } else if evaluated == rebuilt || evaluated == TypeId::ERROR {
                            rebuilt
                        } else {
                            self.materialize_raw_instantiated_constraint(
                                evaluated, memo, active, steps,
                            )
                        }
                    }
                }
                Some(TypeData::Mapped(id)) => {
                    let mapped = self.interner.get_mapped(id);
                    let mapped_constraint = self.materialize_raw_instantiated_constraint(
                        mapped.constraint,
                        memo,
                        active,
                        steps,
                    );
                    let template = self.materialize_raw_instantiated_constraint(
                        mapped.template,
                        memo,
                        active,
                        steps,
                    );
                    let name_type = mapped.name_type.map(|name_type| {
                        self.materialize_raw_instantiated_constraint(name_type, memo, active, steps)
                    });
                    if mapped_constraint == mapped.constraint
                        && template == mapped.template
                        && name_type == mapped.name_type
                    {
                        constraint
                    } else {
                        self.interner.mapped(crate::types::MappedType {
                            constraint: mapped_constraint,
                            template,
                            name_type,
                            ..mapped
                        })
                    }
                }
                Some(TypeData::IndexAccess(object, index)) => {
                    let object =
                        self.materialize_raw_instantiated_constraint(object, memo, active, steps);
                    let index =
                        self.materialize_raw_instantiated_constraint(index, memo, active, steps);
                    let rebuilt = self.interner.index_access(object, index);
                    let evaluated = self.checker.evaluate_type(rebuilt);
                    if evaluated == rebuilt || evaluated == TypeId::ERROR {
                        rebuilt
                    } else {
                        self.materialize_raw_instantiated_constraint(evaluated, memo, active, steps)
                    }
                }
                Some(TypeData::KeyOf(inner)) => {
                    let inner =
                        self.materialize_raw_instantiated_constraint(inner, memo, active, steps);
                    let rebuilt = self.interner.keyof(inner);
                    let evaluated = self.checker.evaluate_type(rebuilt);
                    if evaluated == rebuilt || evaluated == TypeId::ERROR {
                        rebuilt
                    } else {
                        self.materialize_raw_instantiated_constraint(evaluated, memo, active, steps)
                    }
                }
                Some(TypeData::StringIntrinsic { kind, type_arg }) => {
                    let type_arg =
                        self.materialize_raw_instantiated_constraint(type_arg, memo, active, steps);
                    let rebuilt = self.interner.string_intrinsic(kind, type_arg);
                    let evaluated = self.checker.evaluate_type(rebuilt);
                    if evaluated == rebuilt || evaluated == TypeId::ERROR {
                        rebuilt
                    } else {
                        self.materialize_raw_instantiated_constraint(evaluated, memo, active, steps)
                    }
                }
                _ => constraint,
            }
        };
        active.remove(&constraint);
        memo.insert(constraint, materialized);
        materialized
    }

    pub(crate) fn top_rest_any_callable_constraint(&self, constraint: TypeId) -> bool {
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(constraint)
            && let Some(constraint) = tp.constraint
        {
            return self.top_rest_any_callable_constraint(constraint);
        }
        let Some(shape) = Self::get_contextual_signature_cached(self.interner, constraint) else {
            return false;
        };
        if shape.is_constructor || shape.params.len() != 1 || !shape.params[0].rest {
            return false;
        }
        let rest_type = self.unwrap_readonly(shape.params[0].type_id);
        let rest_elem = if let Some(TypeData::Tuple(tuple_id)) = self.interner.lookup(rest_type) {
            let elems = self.interner.tuple_list(tuple_id);
            elems
                .iter()
                .find(|elem| elem.rest)
                .and_then(|elem| {
                    crate::type_queries::get_array_element_type(
                        self.interner.as_type_database(),
                        elem.type_id,
                    )
                    .or(Some(elem.type_id))
                })
                .unwrap_or(rest_type)
        } else {
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), rest_type)
                .unwrap_or(rest_type)
        };
        rest_elem.is_any_or_unknown() && shape.return_type.is_any_or_unknown()
    }

    pub(crate) fn callable_satisfies_top_rest_any_constraint(
        &self,
        candidate: TypeId,
        constraint: TypeId,
    ) -> bool {
        self.top_rest_any_callable_constraint(constraint)
            && Self::get_contextual_signature_cached(self.interner, candidate)
                .is_some_and(|shape| !shape.is_constructor)
    }

    pub(super) fn constrain_types_for_arg_source(
        &mut self,
        arg_index: usize,
        infer_ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        let source = readonly_direct_inference::wrap_readonly_annotation_source(
            self.interner.as_type_database(),
            source,
            self.arg_source_is_readonly_annotation
                .get(arg_index)
                .copied()
                .unwrap_or(false),
        );

        if !self
            .arg_source_is_type_annotation
            .get(arg_index)
            .copied()
            .unwrap_or(false)
        {
            self.constrain_types(infer_ctx, var_map, source, target, priority);
            return;
        }

        let was_type_annotation = infer_ctx.source_is_type_annotation;
        infer_ctx.source_is_type_annotation = true;
        self.constrain_types(infer_ctx, var_map, source, target, priority);
        infer_ctx.source_is_type_annotation = was_type_annotation;
    }

    pub(super) fn generic_rest_tuple_callback_arity_mismatch(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        let rest_param = func.params.last().filter(|param| param.rest)?;
        let rest_type_param =
            self.type_param_index_if_generic_rest_tuple_param(func, rest_param.type_id)?;
        let rest_start = func.params.len().saturating_sub(1);
        let rest_arg_count = arg_types.len().saturating_sub(rest_start);

        for (index, param) in func.params.iter().take(rest_start).enumerate() {
            let Some(target_shape) =
                Self::get_contextual_signature_cached(self.interner, param.type_id)
            else {
                continue;
            };
            let target_shape = self.normalize_function_shape_params_for_context(&target_shape);
            let Some(target_rest) = target_shape.params.last().filter(|param| param.rest) else {
                continue;
            };
            if self.type_param_index_if_generic_rest_tuple_param(func, target_rest.type_id)
                != Some(rest_type_param)
            {
                continue;
            }

            let Some(source_type) = arg_types.get(index).copied() else {
                continue;
            };
            let Some(source_shape) =
                Self::get_contextual_signature_cached(self.interner, source_type)
            else {
                continue;
            };
            let source_shape = self.normalize_function_shape_params_for_context(&source_shape);
            let (callback_min, callback_max) =
                self.arg_count_bounds(&source_shape.params, &source_shape.type_params);

            if rest_arg_count < callback_min || callback_max.is_some_and(|max| rest_arg_count > max)
            {
                return Some(CallResult::ArgumentCountMismatch {
                    expected_min: rest_start + callback_min,
                    expected_max: callback_max.map(|max| rest_start + max),
                    actual: arg_types.len(),
                });
            }
        }

        None
    }

    pub(super) fn apply_callback_optional_rest_slots(
        &mut self,
        func: &FunctionShape,
        final_args: &[TypeId],
        instantiated_params: &mut [ParamInfo],
    ) {
        let Some(raw_rest_param) = func.params.last().filter(|param| param.rest) else {
            return;
        };
        let rest_index = func.params.len().saturating_sub(1);
        let Some(instantiated_rest_param) = instantiated_params.get_mut(rest_index) else {
            return;
        };
        if !instantiated_rest_param.rest {
            return;
        }

        let rest_type = self.unwrap_readonly(instantiated_rest_param.type_id);
        let rest_type = self.evaluate_rest_param_type(rest_type);
        let Some(TypeData::Tuple(elements_id)) = self.interner.lookup(rest_type) else {
            return;
        };
        let mut elements = self.interner.tuple_list(elements_id).to_vec();
        let mut changed = false;

        for (param_index, raw_param) in func.params[..rest_index].iter().enumerate() {
            let Some(target_fn) =
                Self::get_contextual_signature_cached(self.interner, raw_param.type_id)
            else {
                continue;
            };
            let target_uses_same_rest = target_fn
                .params
                .last()
                .is_some_and(|param| param.rest && param.type_id == raw_rest_param.type_id);
            if !target_uses_same_rest {
                continue;
            }

            let Some(&source_arg) = final_args.get(param_index) else {
                continue;
            };
            let Some(source_fn) = Self::get_contextual_signature_cached(self.interner, source_arg)
            else {
                continue;
            };
            let source_params: Vec<ParamInfo> = source_fn
                .params
                .iter()
                .flat_map(|param| {
                    crate::type_queries::unpack_tuple_rest_parameter(self.interner, param)
                })
                .collect();

            for (element, source_param) in elements.iter_mut().zip(source_params.iter()) {
                if source_param.optional && !element.rest && !element.optional {
                    element.optional = true;
                    changed = true;
                }
            }
        }

        if changed {
            instantiated_rest_param.type_id = self.interner.tuple(elements);
        }
    }
}
