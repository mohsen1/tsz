use crate::query_boundaries::common::{TypeSubstitution, instantiate_type, type_param_info};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_assignable_or_report_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.diagnostic_relation_boolean_guard(source, target) {
            return true;
        }

        // Use the canonical assign relation outcome so the weak-union hint is collected alongside
        // the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        let outcome = self.assign_relation_outcome(source, target);
        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    ///
    /// Uses the canonical `RelationRequest` path for combined assignability +
    /// weak-union detection.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(arg_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(arg_idx, arg_idx) {
            return true;
        }
        if target == TypeId::NEVER && self.generic_indexed_access_argument_surface(source) {
            return true;
        }
        let checker_only_mismatch = self
            .checker_only_assignability_failure_reason(source, target)
            .is_some();
        if self.diagnostic_relation_boolean_guard(source, target) && !checker_only_mismatch {
            return true;
        }
        if self.should_suppress_partial_self_argument_mismatch(source, target) {
            return true;
        }
        if self.should_suppress_self_referential_generic_function_arg_mismatch(source, target) {
            return true;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            source, target, arg_idx,
        ) {
            return true;
        }

        // Use the canonical call-argument relation outcome to collect the weak-union hint
        // without a separate solver call.
        let outcome = self.call_arg_relation_outcome(source, target);

        if self.should_skip_weak_union_error_with_outcome(source, target, arg_idx, Some(&outcome)) {
            return true;
        }
        // Conditional/generic callback contexts can narrow argument callback parameter
        // types to intersections involving type parameters (e.g. `number & T`).
        // In these cases, strict contravariant checking reports TS2345 even when the
        // concrete expected callback type is assignable to the narrowed callback.
        // tsc defers this mismatch.
        //
        // Only suppress when the source's parameter types contain type parameters
        // in an intersection with concrete types (indicating narrowing), not when
        // the parameters are standalone type parameters from an enclosing scope.
        // Without this restriction, `(x: T) => void` would be incorrectly accepted
        // for `(x: unknown) => void` just because `T <: unknown` holds in reverse.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            && !crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
            && crate::query_boundaries::assignability::is_callable_type(self.ctx.types, source)
            && crate::query_boundaries::assignability::is_callable_type(self.ctx.types, target)
            && !self.callable_has_own_generic_signatures(source)
            && self.diagnostic_relation_boolean_guard(target, source)
            && self.callable_params_contain_type_param_intersection(source)
        {
            return true;
        }
        // Suppress TS2345 for callbacks with unannotated parameters that rely on
        // contextual typing. When a callback has unannotated parameters, its type
        // depends on the contextual type from the call site. If the contextual
        // typing wasn't properly applied during type inference, the callback's
        // inferred type may not match the expected type, causing false TS2345.
        // This handles cases like JSDoc @enum types where the callback parameter
        // should be contextually typed but the assignability check happens before
        // contextual typing is fully resolved.
        //
        // Only suppress when the target callable can actually contextually type
        // every parameter of the source callback. If the target signature has
        // fewer fixed parameters than the source callback (and no rest
        // parameter), contextual typing cannot supply types for the extra
        // source parameters, and the parameter-count mismatch ("Target
        // signature provides too few arguments") must surface as TS2345.
        if !checker_only_mismatch
            && self.arg_is_callback_with_unannotated_params(arg_idx)
            && self.target_can_contextually_type_callback_params(arg_idx, target)
        {
            return true;
        }
        // Before emitting TS2345 on the whole argument, try to elaborate
        // the error down to specific properties (TS2322) for object/array
        // literal arguments. tsc reports TS2322 on specific mismatched
        // properties rather than TS2345 on the whole argument.
        if self.try_elaborate_assignment_source_error(arg_idx, target) {
            return false;
        }
        if self.try_elaborate_callback_body_diagnostics(arg_idx, target) {
            return false;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }

    pub(crate) fn should_suppress_partial_self_argument_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(inner) = self.partial_self_argument_inner_type(target) else {
            return false;
        };

        self.type_matches_partial_self_inner(source, inner)
    }

    fn partial_self_argument_inner_type(&mut self, target: TypeId) -> Option<TypeId> {
        let (base, args) = self.application_info_or_display_alias(target).or_else(|| {
            let evaluated = self.evaluate_type_for_assignability(target);
            self.application_info_or_display_alias(evaluated)
        })?;
        self.partial_like_application_inner_arg(base, &args)
    }

    fn partial_like_application_inner_arg(&self, base: TypeId, args: &[TypeId]) -> Option<TypeId> {
        if args.len() == 1 && self.application_base_is_lib_partial(base) {
            return args.first().copied();
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || def.type_params.len() != args.len() {
            return None;
        }
        let inner = self.optional_homomorphic_mapped_inner_type(def.body?)?;
        let param = type_param_info(self.ctx.types, inner)?;
        let arg_idx = def
            .type_params
            .iter()
            .position(|type_param| type_param.name == param.name)?;
        args.get(arg_idx).copied()
    }

    fn optional_homomorphic_mapped_inner_type(&self, type_id: TypeId) -> Option<TypeId> {
        let mapped = crate::query_boundaries::common::mapped_type_info(self.ctx.types, type_id)?;
        if mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove)
            || mapped.optional_modifier.is_none()
        {
            return None;
        }

        let inner =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, mapped.constraint)?;
        let (template_object, _) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, mapped.template)?;
        (template_object == inner).then_some(inner)
    }

    fn application_base_is_lib_partial(&self, base: TypeId) -> bool {
        let Some(partial_def) = self.ctx.actual_lib_def_id_for_bare_name("Partial") else {
            return false;
        };
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
            == Some(partial_def)
    }

    fn type_matches_partial_self_inner(&mut self, source: TypeId, inner: TypeId) -> bool {
        if source == inner {
            return true;
        }
        self.ctx.types.get_display_alias(source) == Some(inner)
            || self.partial_inner_alias_instantiates_to_source(inner, source)
    }

    fn partial_inner_alias_instantiates_to_source(
        &mut self,
        inner: TypeId,
        source: TypeId,
    ) -> bool {
        let Some((base, args)) = self.application_info_or_display_alias(inner) else {
            return false;
        };
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias || def.type_params.len() != args.len() {
            return false;
        }
        let Some(body) = def.body else {
            return false;
        };

        let substitution = TypeSubstitution::from_args(self.ctx.types, &def.type_params, &args);
        let instantiated = instantiate_type(self.ctx.types, body, &substitution);
        source == instantiated || self.ctx.types.get_display_alias(source) == Some(instantiated)
    }

    fn should_suppress_self_referential_generic_function_arg_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(source_sig) = crate::query_boundaries::common::callable_shape_for_type_extended(
            self.ctx.types,
            source,
        )
        .and_then(|shape| {
            (shape.call_signatures.len() == 1).then(|| shape.call_signatures[0].clone())
        }) else {
            return false;
        };
        if !source_sig.type_params.iter().any(|tp| {
            tp.constraint.is_some_and(|constraint| {
                crate::query_boundaries::common::contains_type_parameter_named(
                    self.ctx.types,
                    constraint,
                    tp.name,
                )
            })
        }) {
            return false;
        }

        let Some(target_sig) = crate::query_boundaries::common::callable_shape_for_type_extended(
            self.ctx.types,
            target,
        )
        .and_then(|shape| {
            (shape.call_signatures.len() == 1).then(|| shape.call_signatures[0].clone())
        }) else {
            return false;
        };
        if target_sig.return_type != TypeId::UNKNOWN {
            return false;
        }
        let Some(rest_param) = target_sig.params.last().filter(|param| param.rest) else {
            return false;
        };
        if rest_param.type_id == TypeId::UNKNOWN {
            return true;
        }
        crate::query_boundaries::common::tuple_elements(self.ctx.types, rest_param.type_id)
            .is_some_and(|elements| {
                !elements.is_empty()
                    && elements
                        .iter()
                        .all(|element| element.type_id == TypeId::UNKNOWN)
            })
    }

    pub(crate) fn should_suppress_self_referential_mapped_constraint_arg_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if self
            .ctx
            .arena
            .get(arg_idx)
            .is_none_or(|node| node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            return false;
        }
        if !crate::query_boundaries::diagnostics::contains_type_parameters(self.ctx.types, target)
            || !self.type_contains_generic_mapped_constraint(target, &mut Default::default())
        {
            return false;
        }

        let mut substitution = TypeSubstitution::new();
        for referenced in
            crate::query_boundaries::diagnostics::collect_referenced_types(self.ctx.types, target)
        {
            let Some(info) = type_param_info(self.ctx.types, referenced) else {
                continue;
            };
            let Some(constraint) = info.constraint else {
                if crate::query_boundaries::diagnostics::contains_type_parameter_named(
                    self.ctx.types,
                    target,
                    info.name,
                ) {
                    substitution.insert(info.name, source);
                }
                continue;
            };
            if crate::query_boundaries::diagnostics::contains_type_parameter_named(
                self.ctx.types,
                constraint,
                info.name,
            ) || crate::query_boundaries::diagnostics::contains_type_parameter_named(
                self.ctx.types,
                target,
                info.name,
            ) {
                substitution.insert(info.name, source);
            }
        }
        if substitution.is_empty() {
            return false;
        }

        let instantiated = instantiate_type(self.ctx.types, target, &substitution);
        let env_evaluated = self.evaluate_type_with_env(instantiated);
        let evaluated = self.evaluate_type_for_assignability(env_evaluated);
        let contextual = self.evaluate_contextual_type(instantiated);
        evaluated != target
            && evaluated != TypeId::UNKNOWN
            && evaluated != TypeId::ERROR
            && (self.diagnostic_relation_boolean_guard_with_env(source, evaluated)
                || self.diagnostic_relation_boolean_guard_with_env(source, contextual)
                || self.self_referential_mapped_intersection_accepts_object_literal(
                    source, evaluated, arg_idx,
                ))
    }

    fn self_referential_mapped_intersection_accepts_object_literal(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)
        else {
            return false;
        };

        let mut skipped_generic_mapped = false;
        let mut allowed_keys = rustc_hash::FxHashSet::default();
        for member in members {
            if self.type_contains_generic_mapped_constraint(member, &mut Default::default())
                || crate::query_boundaries::common::mapped_type_info(self.ctx.types, member)
                    .is_some()
            {
                skipped_generic_mapped = true;
                continue;
            }

            let member = self.evaluate_type_with_env(member);
            let Some(shape) =
                crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, member)
            else {
                if !self.diagnostic_relation_boolean_guard_with_env(source, member) {
                    return false;
                }
                continue;
            };

            allowed_keys.extend(shape.properties.iter().map(|prop| prop.name));
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return self.diagnostic_relation_boolean_guard_with_env(source, member);
            }
            if !self.diagnostic_relation_boolean_guard_with_env(source, member) {
                return false;
            }
        }

        skipped_generic_mapped
            && self
                .object_literal_property_names(arg_idx)
                .is_some_and(|names| names.into_iter().all(|name| allowed_keys.contains(&name)))
    }

    fn object_literal_property_names(&self, arg_idx: NodeIndex) -> Option<Vec<tsz_common::Atom>> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.ctx.arena.get_literal_expr(node)?;
        let mut names = Vec::new();
        for &element_idx in &object.elements.nodes {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let name = if let Some(prop) = self.ctx.arena.get_property_assignment(element) {
                self.get_property_name(prop.name)
            } else if element.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                self.ctx
                    .arena
                    .get_shorthand_property(element)
                    .and_then(|prop| self.ctx.arena.get_identifier_text(prop.name))
                    .map(str::to_string)
            } else if let Some(method) = self.ctx.arena.get_method_decl(element) {
                self.property_name_for_error(method.name)
            } else {
                None
            };
            let name = name?;
            names.push(self.ctx.types.intern_string(&name));
        }
        Some(names)
    }

    fn type_contains_generic_mapped_constraint(
        &self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }
        if crate::query_boundaries::common::is_generic_mapped_type(self.ctx.types, type_id) {
            return true;
        }
        if let Some(mapped) =
            crate::query_boundaries::common::mapped_type_info(self.ctx.types, type_id)
        {
            return self.type_contains_generic_mapped_constraint(mapped.constraint, visited)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_generic_mapped_constraint(name_type, visited)
                });
        }
        if let Some((_, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            && args
                .iter()
                .any(|&arg| self.type_contains_generic_mapped_constraint(arg, visited))
        {
            return true;
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            && members
                .iter()
                .any(|&member| self.type_contains_generic_mapped_constraint(member, visited))
        {
            return true;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
            && members
                .iter()
                .any(|&member| self.type_contains_generic_mapped_constraint(member, visited))
        {
            return true;
        }
        if let Some((object_type, index_type)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, type_id)
        {
            return self.type_contains_generic_mapped_constraint(object_type, visited)
                || self.type_contains_generic_mapped_constraint(index_type, visited);
        }
        if let Some(info) = type_param_info(self.ctx.types, type_id)
            && let Some(constraint) = info.constraint
        {
            return self.type_contains_generic_mapped_constraint(constraint, visited);
        }
        false
    }

    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        if self
            .checker_only_assignability_failure_reason(source, target)
            .is_some()
        {
            return true;
        }

        // Strict relation first: keep the base method's method-local generics
        // universally quantified (`NO_ERASE_GENERICS`) so a concrete override
        // cannot silently satisfy a generic base signature. This mirrors the
        // `implements` member-override path (`should_report_own_member_type_mismatch`),
        // which uses `is_assignable_to_no_erase_generics`, while preserving the
        // bivariant parameter behavior that class method overrides require.
        if self.is_assignable_to_bivariant_no_erase_generics(source, target) {
            return false;
        }

        // Safe erasure fallback: tsc only canonicalizes (erases) the target's
        // method-local type parameters when the *source* signature carries its
        // own (or the target has none). In those cases the generic-erasing
        // bivariant relation is the correct authority — e.g. `any`-propagation
        // shapes and generic-to-generic overrides where both sides quantify.
        if crate::query_boundaries::class::generic_erasure_fallback_is_safe(self, source, target)
            && self.diagnostic_relation_boolean_guard_bivariant(source, target)
        {
            return false;
        }

        // Route the weak-union check through RelationRequest with the
        // BivariantCallbacks kind so the pre-computed outcome avoids a
        // redundant solver round-trip in the fallback path.
        //
        // `should_skip_weak_union_error_with_outcome` is the sole authority
        // over the weak-union skip decision - including for `weak_union_violation`
        // cases. Do not add an outer `!outcome.weak_union_violation` gate here;
        // that guard suppresses TS2416 for non-object-literal sources (e.g.
        // class property declarations) where the skip should NOT fire.
        let outcome = self.bivariant_callbacks_relation_outcome(source, target);
        !self.should_skip_weak_union_error_with_outcome(source, target, source_idx, Some(&outcome))
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.diagnostic_relation_boolean_guard(left, right)
            && self.diagnostic_relation_boolean_guard(right, left)
    }
}
