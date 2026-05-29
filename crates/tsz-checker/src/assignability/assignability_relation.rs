//! Assignability relation execution and relation-specific fast paths.

use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, are_types_overlapping_with_env, assignability_cache_key,
    check_application_variance_assignability, get_allowed_keys, get_keyof_type,
    get_string_literal_value, get_union_members, intersection_source_has_target_constituent,
    is_assignable_bivariant_with_resolver, is_assignable_with_overrides, is_relation_cacheable,
    object_shape_for_type,
};
use crate::query_boundaries::common::{
    intersection_members, object_shape_id, object_with_index_shape_id, union_members,
};
use crate::query_boundaries::state::type_resolution::get_lazy_def_id;
use crate::state::{CheckerOverrideProvider, CheckerState};
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_solver::TypeId;
use tsz_solver::computation::TypeResolver;

impl<'a> CheckerState<'a> {
    /// Shared assignability core: cache lookup → compute → cache insert → trace.
    ///
    /// Callers prepare evaluated source/target and supply `extra_flags` to OR
    /// into the base relation flags. This eliminates the duplicated
    /// cache+compute+trace sandwich from `is_assignable_to`, `_strict`, and
    /// `_strict_null`.
    fn check_assignability_cached(
        &mut self,
        source: TypeId,
        target: TypeId,
        extra_flags: u16,
        label: &str,
    ) -> bool {
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);
        let flags = self.ctx.pack_relation_flags() | extra_flags;

        if is_cacheable {
            let cache_key = assignability_cache_key(source, target, flags);
            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let overrides = CheckerOverrideProvider::new(self, None);
        let relation_result = is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        );
        let result = relation_result.is_related();

        self.propagate_overflow_flags(
            relation_result.depth_exceeded,
            relation_result.iteration_exceeded,
        );

        if is_cacheable {
            let cache_key = assignability_cache_key(source, target, flags);
            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(source = source.0, target = target.0, result, "{label}");
        result
    }

    fn namespace_source_has_matching_property_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.ctx.namespace_module_names.contains_key(&source) {
            return false;
        }
        if let Some(members) = get_union_members(self.ctx.types, target) {
            return members.iter().all(|&member| {
                self.namespace_source_has_matching_property_mismatch(source, member)
            });
        }
        let Some(shape) = object_shape_for_type(self.ctx.types, source) else {
            return false;
        };
        let source_props = shape.properties.clone();
        let target_eval = self.evaluate_type_for_assignability(target);
        let target_resolved = self.resolve_lazy_type(target_eval);
        let target_with_resolution = self.evaluate_type_with_resolution(target);
        let target_resolver_resolved = get_lazy_def_id(self.ctx.types, target)
            .and_then(|def_id| {
                <crate::context::CheckerContext<'_> as TypeResolver>::resolve_lazy(
                    &self.ctx,
                    def_id,
                    self.ctx.types,
                )
            })
            .unwrap_or(target_resolved);
        let target_shape = object_shape_for_type(self.ctx.types, target_resolver_resolved)
            .or_else(|| object_shape_for_type(self.ctx.types, target_with_resolution))
            .or_else(|| object_shape_for_type(self.ctx.types, target_resolved))
            .or_else(|| object_shape_for_type(self.ctx.types, target_eval))
            .or_else(|| object_shape_for_type(self.ctx.types, target));
        let Some(target_shape) = target_shape else {
            return true;
        };

        target_shape.properties.iter().any(|target_prop| {
            source_props
                .iter()
                .find(|source_prop| source_prop.name == target_prop.name)
                .is_some_and(|source_prop| {
                    !self.is_assignable_to(source_prop.type_id, target_prop.type_id)
                })
        })
    }

    /// Prepare inputs common to all non-bivariant assignability checks:
    /// resolve lazy refs, substitute `ThisType`, and evaluate both sides.
    pub(crate) fn prepare_assignability_inputs(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (TypeId, TypeId) {
        self.ensure_relation_inputs_ready(&[source, target]);
        let raw_source = self.substitute_this_type_if_needed(source);
        let raw_target = self.substitute_this_type_if_needed(target);
        let source = self.evaluate_type_for_assignability(raw_source);
        let target = self.evaluate_type_for_assignability(raw_target);
        (source, target)
    }

    /// Execute a `RelationRequest` through the canonical boundary, returning
    /// a structured `RelationOutcome`.
    ///
    /// This is the single authoritative checker-level entry point for relation
    /// queries that need both the assignability result AND structured failure
    /// information. It replaces the pattern of calling `is_assignable_to` +
    /// `analyze_assignability_failure` + `is_weak_union_violation` separately.
    ///
    /// The request must contain **prepared** (evaluated) source/target types.
    pub(crate) fn execute_relation_request(
        &mut self,
        request: &crate::query_boundaries::assignability::RelationRequest,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        use crate::query_boundaries::assignability::execute_relation;

        let flags = self.ctx.pack_relation_flags();

        if self
            .homomorphic_mapped_display_source_assignable_to_target(request.source, request.target)
            || self.callable_source_satisfies_union_callable_arm(request.source, request.target)
        {
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }

        let overrides = CheckerOverrideProvider::new(self, None);

        let mut outcome = execute_relation(
            request,
            self.ctx.types,
            &self.ctx,
            flags,
            &self.ctx.inheritance_graph,
            &overrides,
            Some(&self.ctx),
            self.ctx.sound_mode(),
        );

        self.propagate_overflow_flags(outcome.depth_exceeded, outcome.iteration_exceeded);

        // Checker-only post-check: the solver may say "related" but the checker
        // can downgrade via deferred conditional types or other checker-specific
        // semantic rules.
        if outcome.related
            && self
                .checker_only_assignability_failure_reason(request.source, request.target)
                .is_some()
        {
            outcome.related = false;
        }

        outcome
    }

    /// Execute a diagnostic-bearing assignment relation for raw checker types.
    ///
    /// This keeps diagnostic code on the `RelationRequest`/`RelationOutcome`
    /// path without repeating the prepare/build/execute boilerplate at each
    /// TS2322-family call site.
    pub(crate) fn assign_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let related = self.is_assignable_to(source, target);
        if related {
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }

        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::assign(source, target);
        let mut outcome = self.execute_relation_request(&request);
        outcome.related = false;
        outcome
    }

    /// Execute a diagnostic-bearing assignment relation using the current
    /// `TypeEnvironment`, preserving the no-cache semantics of
    /// `is_assignable_to_with_env` while returning a structured outcome.
    pub(crate) fn assign_relation_outcome_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let outcome = |related| crate::query_boundaries::assignability::RelationOutcome {
            related,
            depth_exceeded: false,
            iteration_exceeded: false,
            failure: None,
            weak_union_violation: false,
            property_classification: None,
        };

        if source == target || self.is_assignable_to_with_env(source, target) {
            return outcome(true);
        }

        self.ensure_relation_inputs_ready(&[source, target]);
        let target = self.substitute_this_type_if_needed(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return outcome(false);
        }

        {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &*env,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return outcome(result);
            }
        }

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let mut relation_outcome = {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let overrides = CheckerOverrideProvider::new(self, Some(&*env));
            let request =
                crate::query_boundaries::assignability::RelationRequest::assign(source, target);
            crate::query_boundaries::assignability::execute_relation(
                &request,
                self.ctx.types,
                &*env,
                flags,
                &self.ctx.inheritance_graph,
                &overrides,
                Some(&self.ctx),
                self.ctx.sound_mode(),
            )
        };

        self.propagate_overflow_flags(
            relation_outcome.depth_exceeded,
            relation_outcome.iteration_exceeded,
        );

        if relation_outcome.related
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            relation_outcome.related = false;
        }

        if relation_outcome.related
            && let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            if !allowed_keys.is_empty() && !allowed_keys.contains(&source_str) {
                relation_outcome.related = false;
            }
        }

        relation_outcome.related = false;
        relation_outcome
    }

    /// Execute a diagnostic-bearing call-argument relation for raw checker
    /// types, preserving the canonical TS2345 relation path.
    pub(crate) fn call_arg_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_arg(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing bivariant-callback relation for raw
    /// checker types, preserving the canonical callback relation path.
    pub(crate) fn bivariant_callbacks_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::bivariant_callbacks(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Boolean relation guard for diagnostic code paths.
    ///
    /// Keep these calls grep-distinct from diagnostic decisions that need
    /// `RelationOutcome` failure classification, weak-union handling, or depth
    /// reporting.
    pub(crate) fn diagnostic_relation_boolean_guard(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Environment-aware boolean relation guard for diagnostic code paths.
    ///
    /// Use this only when the caller intentionally needs the current
    /// `TypeEnvironment` and no relation-cache lookup.
    pub(crate) fn diagnostic_relation_boolean_guard_with_env(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.is_assignable_to_with_env(source, target)
    }

    /// Bivariant-callback boolean relation guard for diagnostic code paths.
    ///
    /// Use this only when the caller intentionally needs the legacy bivariant
    /// callback relation rather than the default assignability relation.
    pub(crate) fn diagnostic_relation_boolean_guard_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.is_assignable_to_bivariant(source, target)
    }

    /// No-weak-checks boolean relation guard for diagnostic code paths.
    ///
    /// Use this only when the caller intentionally mirrors `tsc`'s
    /// `isTypeAssignableTo` path without TS2559 weak-type detection.
    pub(crate) fn diagnostic_relation_boolean_guard_no_weak_checks(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.is_assignable_to_no_weak_checks(source, target)
    }

    /// Check if source type is assignable to target type.
    ///
    /// This is the main entry point for assignability checking, used throughout
    /// the type system to validate assignments, function calls, returns, etc.
    /// Assignability is more permissive than subtyping.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let mut source = self.substitute_this_type_if_needed(source);
        let mut target = self.substitute_this_type_if_needed(target);
        let raw_source = source;
        let raw_target = target;
        source = self.normalize_awaited_application_args_for_variance(source);
        target = self.normalize_awaited_application_args_for_variance(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return false;
        }

        // Inference-fallback fast path for same-base Application types:
        // when the source is `FooPromise<unknown, ...>` (all args `unknown`)
        // and the target is the same promise-like generic with at least one
        // `never` arg and no concrete non-`never` args, treat the source as
        // assignable.
        //
        // This handles the common Thenable / Promise inference pattern where
        // a constructor call cannot infer type parameters used only in nested
        // applications (e.g., `new EPromise(Promise.resolve(mkRight(a)))`
        // where `EPromise<E, A>` takes `PromiseLike<Either<E, A>>`). Without
        // explicit type args, the result is `EPromise<unknown, unknown>`,
        // which must still be assignable to a declared return type like
        // `EPromise<never, A>`.
        //
        // The promise-like and target-arg requirements keep this fast path
        // narrow: it doesn't match user-written `A<unknown>` against
        // `A<string>`, `A<never>`, or `A<never, string>`, where variance must
        // be respected.
        if self.is_unknown_source_application_fallback(source, target) {
            return true;
        }

        if self.is_nested_same_wrapper_application_assignment(source, target) {
            return true;
        }

        if self.homomorphic_mapped_display_source_assignable_to_target(source, target) {
            return true;
        }

        // Variance-aware fast path: when both source and target are Application
        // types with the same base (e.g., Covariant<A> vs Covariant<B>), check
        // type arguments using computed variance BEFORE structural expansion.
        // This must run before evaluate_type_for_assignability which would
        // expand Application types to structural objects, losing variance info.
        {
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        if self.same_base_application_to_constrained_type_param_target(source, target) {
            return false;
        }

        if self.same_type_alias_application_args_reject(source, target) {
            return false;
        }

        // Pre-evaluation IndexAccess identity check: when both source and target are
        // IndexAccess types whose object types are the same type parameter identity,
        // accept the relationship before evaluation can destroy type parameter identity.
        // Example: `T_229[K] <: T_420[K]` where T_229 (unconstrained, from type alias)
        // and T_420 (constrained `extends object`, from function) share name "T".
        // Without this, evaluation resolves T_420 to `object`, losing the name match.
        if let Some((s_obj, s_idx)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                source,
            )
            && let Some((t_obj, t_idx)) =
                crate::query_boundaries::checkers::generic::index_access_components(
                    self.ctx.types,
                    target,
                )
            && crate::query_boundaries::common::type_param_info(self.ctx.types, s_obj).is_some()
            && crate::query_boundaries::common::type_param_info(self.ctx.types, t_obj).is_some()
            && self.type_parameter_identities_match(s_obj, t_obj)
            && self.is_generic_index_key_assignable(s_idx, t_idx)
        {
            return true;
        }

        // Pre-evaluation IndexAccess covariance check: `U[J]` is assignable to
        // `T[K]` when `U extends T` and `J extends K`. Evaluation can erase the
        // source object identity before the key relationship is considered.
        if let Some((s_obj, s_idx)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                source,
            )
            && let Some((t_obj, t_idx)) =
                crate::query_boundaries::checkers::generic::index_access_components(
                    self.ctx.types,
                    target,
                )
            && self.type_param_constraint_chain_reaches(s_obj, t_obj)
            && self.is_generic_index_key_assignable(s_idx, t_idx)
        {
            return true;
        }

        // Pre-evaluation IndexAccess object-constraint rejection: `T[K]` is not
        // assignable to `U[K]` when `U extends T`. Evaluating through U's
        // constraint can erase that distinction and make both sides look like
        // `T[K]`, but U may be instantiated with narrower property values.
        if let Some((s_obj, s_idx)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                source,
            )
            && let Some((t_obj, t_idx)) =
                crate::query_boundaries::checkers::generic::index_access_components(
                    self.ctx.types,
                    target,
                )
            && self.is_assignable_to(s_idx, t_idx)
            && let Some(t_param) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, t_obj)
            && t_param.constraint.is_some_and(|constraint| {
                constraint == s_obj
                    || (crate::query_boundaries::common::type_param_info(
                        self.ctx.types,
                        constraint,
                    )
                    .is_some()
                        && crate::query_boundaries::common::type_param_info(self.ctx.types, s_obj)
                            .is_some()
                        && self.type_parameter_identities_match(constraint, s_obj))
            })
        {
            return false;
        }

        // Pre-evaluation IndexAccess key-identity rejection: when both source and
        // target are `O[K]` types with the same object type O but different generic
        // type-parameter keys, reject before evaluation. Eager evaluation of `O[T_s]`
        // and `O[T_t]` resolves both to the same value-union derived from the
        // shared constraint, which loses the per-call-site type-param identity that
        // tsc preserves when reporting TS2322 ("`T_t` could be instantiated with a
        // different subtype of constraint `keyof O`"). Without this guard, the
        // assignability check trivially succeeds via `source_eval == target_eval`.
        if let Some((s_obj, s_idx)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                source,
            )
            && let Some((t_obj, t_idx)) =
                crate::query_boundaries::checkers::generic::index_access_components(
                    self.ctx.types,
                    target,
                )
            && s_obj == t_obj
            && crate::query_boundaries::common::type_param_info(self.ctx.types, s_idx).is_some()
            && crate::query_boundaries::common::type_param_info(self.ctx.types, t_idx).is_some()
            && !self.type_parameter_identities_match(s_idx, t_idx)
        {
            return false;
        }

        if let Some(concrete_target) = self.concrete_remapped_mapped_assignability_target(target) {
            return self.is_assignable_to(source, concrete_target);
        }

        source = self.normalize_index_access_for_assignability(source, 0);
        target = self.normalize_index_access_for_assignability(target, 0);

        if intersection_source_has_target_constituent(self.ctx.types, source, target) {
            return true;
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);

        // Guard: if evaluation degraded a valid type to ERROR (e.g., due to the
        // stack overflow protection tripping during deep recursive type resolution),
        // preserve the pre-evaluation type. ERROR is treated as assignable to/from
        // everything by the subtype checker, which would silently suppress real type
        // errors (like TS2322 for property mismatches in object literals with
        // recursive interface targets). Keeping the original Lazy type allows the
        // compat checker's resolver to resolve it from the type environment, which
        // was populated during earlier successful resolution.
        let source = if source_eval == TypeId::ERROR && source != TypeId::ERROR {
            source
        } else {
            source_eval
        };
        let target = if target_eval == TypeId::ERROR && target != TypeId::ERROR {
            target
        } else {
            target_eval
        };

        if self.callable_source_satisfies_union_callable_arm(source, target) {
            return true;
        }

        if let (Some(s_elem), Some(t_elem)) = (
            crate::query_boundaries::common::array_element_type(self.ctx.types, source),
            crate::query_boundaries::common::array_element_type(self.ctx.types, target),
        ) {
            if self.same_type_alias_application_args_reject(s_elem, t_elem) {
                return false;
            }
            if s_elem == TypeId::ERROR
                && self.static_schema_application_schema_type(t_elem).is_some()
            {
                return false;
            }
            let s_elem_normalized = self.evaluate_awaited_application_for_assignability(s_elem);
            let t_elem_normalized = self.evaluate_awaited_application_for_assignability(t_elem);
            if s_elem_normalized != s_elem || t_elem_normalized != t_elem {
                return self.is_assignable_to(s_elem_normalized, t_elem_normalized);
            }
            if !self.is_assignable_to(s_elem, t_elem) {
                return false;
            }
        }

        let result = self.check_assignability_cached(source, target, 0, "is_assignable_to");

        if result && self.same_type_alias_application_args_reject(source, target) {
            return false;
        }

        if result
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }

        if result && self.namespace_source_has_matching_property_mismatch(source, target) {
            return false;
        }

        if !result && self.is_assignable_with_target_this_bound_to_source(raw_source, raw_target) {
            return true;
        }

        // Post-check: keyof type checking logic
        if let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            // Only reject when we could determine concrete keys. An empty set means
            // the inner type couldn't be resolved (e.g., ThisType, TypeParameter,
            // or Application). In that case, trust the solver's result.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    pub(crate) fn type_predicate_type_assignable_to_parameter(
        &mut self,
        predicate_type: TypeId,
        param_type: TypeId,
    ) -> bool {
        let types = self.ctx.types;
        crate::query_boundaries::type_predicates::type_predicate_type_assignable_to_parameter_with(
            types,
            predicate_type,
            param_type,
            |source, target| self.is_assignable_to(source, target),
        )
    }

    fn is_assignable_with_target_this_bound_to_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source.is_intrinsic() || target.is_intrinsic() {
            return false;
        }

        // In a target interface, polymorphic `this` stands for the concrete
        // subtype being assigned to that interface, not the interface itself.
        let rebound_target =
            if crate::query_boundaries::common::contains_this_type(self.ctx.types, target) {
                crate::query_boundaries::common::substitute_this_type(
                    self.ctx.types,
                    target,
                    source,
                )
            } else if let Some(rebound) =
                self.instantiate_application_target_this_bound_to_source(target, source)
            {
                rebound
            } else {
                return false;
            };
        if rebound_target == target {
            return false;
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(rebound_target);
        let source = if source_eval == TypeId::ERROR && source != TypeId::ERROR {
            source
        } else {
            source_eval
        };
        let target = if target_eval == TypeId::ERROR && rebound_target != TypeId::ERROR {
            rebound_target
        } else {
            target_eval
        };

        self.check_assignability_cached(source, target, 0, "target_this_bound_to_source")
    }

    fn instantiate_application_target_this_bound_to_source(
        &mut self,
        target: TypeId,
        source: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, target)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let (body_type, type_params) = {
            let env = self.ctx.type_env.borrow();
            let body_type = TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)?;
            let type_params = TypeResolver::get_lazy_type_params(&*env, def_id).unwrap_or_default();
            (body_type, type_params)
        };
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &type_params,
            &app.args,
        );
        let (mut instantiated, depth_exceeded) =
            crate::query_boundaries::common::instantiate_type_with_depth_status(
                self.ctx.types,
                body_type,
                &substitution,
            );
        if depth_exceeded {
            self.ctx.depth_exceeded.set(true);
        }
        if !crate::query_boundaries::common::contains_this_type(self.ctx.types, instantiated) {
            return None;
        }
        instantiated = crate::query_boundaries::common::substitute_this_type(
            self.ctx.types,
            instantiated,
            source,
        );
        Some(self.evaluate_type_for_assignability(instantiated))
    }

    fn same_type_alias_application_args_reject(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some((source_base, source_args)) = self.application_display_info(source) else {
            return false;
        };
        let Some((target_base, target_args)) = self.application_display_info(target) else {
            return false;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return false;
        }
        if source_args
            .iter()
            .zip(target_args.iter())
            .all(|(s, t)| s == t)
        {
            return false;
        }
        let Some(def_id) =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, source_base)
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return false;
        }
        if self.type_alias_args_are_unwitnessed(def_id, source_args.len()) {
            return false;
        }
        if self.type_alias_projects_static_member(source_base) {
            return true;
        }
        let variances = tsz_solver::relations::variance::compute_type_param_variances_with_resolver(
            self.ctx.types.as_type_database(),
            &self.ctx,
            def_id,
        );
        source_args.iter().zip(target_args.iter()).enumerate().any(
            |(i, (&source_arg, &target_arg))| {
                if target_arg.is_any() {
                    return false;
                }
                let variance = variances.as_ref().and_then(|vs| vs.get(i)).copied();
                match variance {
                    // Variance is unreliable or requires a structural fallback — the
                    // structural check is authoritative; don't force a rejection here.
                    Some(v) if v.rejection_unreliable() || v.needs_structural_fallback() => false,
                    // K in `{ [P in K]: V }` is CONTRAVARIANT: a source with wider keys
                    // covers a target with narrower keys, so reverse the direction.
                    Some(v) if v.is_contravariant() => {
                        !self.is_assignable_to(target_arg, source_arg)
                    }
                    // Covariant or unknown: source must be assignable to target.
                    _ => !self.is_assignable_to(source_arg, target_arg),
                }
            },
        )
    }

    fn type_alias_args_are_unwitnessed(
        &self,
        def_id: tsz_solver::def::DefId,
        arg_len: usize,
    ) -> bool {
        tsz_solver::relations::variance::compute_type_param_variances_with_resolver(
            self.ctx.types.as_type_database(),
            &self.ctx,
            def_id,
        )
        .as_ref()
        .is_some_and(|variances| {
            variances.len() == arg_len && variances.iter().all(|v| v.is_independent())
        })
    }

    fn application_display_info(&self, type_id: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
        self.application_info_or_display_alias(type_id)
    }

    fn homomorphic_mapped_display_source_assignable_to_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_display = self.application_display_info(source);
        let target_display = self.application_display_info(target);
        let source = source_display
            .map(|(base, args)| self.ctx.types.application(base, args))
            .unwrap_or(source);
        let target = target_display
            .map(|(base, args)| self.ctx.types.application(base, args))
            .unwrap_or(target);
        crate::query_boundaries::assignability::homomorphic_mapped_source_assignable_to_target(
            self.ctx.types,
            &self.ctx,
            source,
            target,
        )
    }

    /// Type assertion overlap uses tsc's comparable relation, not ordinary
    /// assignment. In particular, method bivariance must not make distinct
    /// generic instantiations appear to overlap.
    pub(crate) fn is_assignable_for_type_assertion_overlap(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);

        {
            let flags = self.ctx.pack_relation_flags()
                | crate::query_boundaries::assignability::RelationFlags::DISABLE_METHOD_BIVARIANCE;
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);
        let source = if source_eval == TypeId::ERROR && source != TypeId::ERROR {
            source
        } else {
            source_eval
        };
        let target = if target_eval == TypeId::ERROR && target != TypeId::ERROR {
            target
        } else {
            target_eval
        };
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::DISABLE_METHOD_BIVARIANCE,
            "is_assignable_for_type_assertion_overlap",
        )
    }

    fn is_concrete_source_to_deferred_keyof_index_access(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((object_type, index_type)) =
            crate::query_boundaries::checkers::generic::index_access_components(
                self.ctx.types,
                target,
            )
        else {
            return false;
        };

        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
        {
            return false;
        }

        if !self.is_deferred_generic_index_for_object(index_type, object_type) {
            return false;
        }

        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, object_type) {
            return source != TypeId::ANY;
        }

        let mut candidate_types = Vec::new();
        self.collect_deferred_index_access_candidate_types(object_type, &mut candidate_types);

        if candidate_types.is_empty() {
            return crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                object_type,
            );
        }

        // Structural fast path for `{}` source: avoids re-evaluating every
        // candidate's generic application through the full relation, which
        // can degrade to a false negative when evaluation fuel is exhausted
        // on one candidate of a 100+-key intrinsic map.
        if crate::query_boundaries::common::is_empty_object_type(self.ctx.types, source) {
            let mut visited = FxHashSet::default();
            return candidate_types
                .iter()
                .any(|&candidate| self.candidate_rejects_empty_object(candidate, &mut visited));
        }

        // Use the checker's compat-aware `is_assignable_to`, not the solver's
        // strict subtype check. The Lawyer (CompatChecker) accepts permissive
        // cases that the Judge (SubtypeChecker) rejects — most importantly,
        // `{}` is assignable to any object type with all-optional properties
        // (e.g. `BaseProps<T> { id?: string }`). Routing through the strict
        // subtype check produced false-positive TS2322 on `let x: O[K] = {}`
        // where K is a deferred generic key and O has all-optional value
        // properties — tsc accepts this.
        candidate_types
            .into_iter()
            .any(|candidate| !self.is_assignable_to(source, candidate))
    }

    /// `true` iff `{}` would be rejected against `candidate`. Falls back to
    /// `false` when the shape cannot be inspected, so an inconclusive probe
    /// here cannot manufacture a false positive against the caller.
    fn candidate_rejects_empty_object(
        &mut self,
        candidate: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if candidate == TypeId::ANY
            || candidate == TypeId::UNKNOWN
            || candidate == TypeId::NEVER
            || candidate == TypeId::ERROR
            || candidate == TypeId::NULL
            || candidate == TypeId::UNDEFINED
            || candidate == TypeId::VOID
        {
            return false;
        }
        if !visited.insert(candidate) {
            return false;
        }

        let evaluated = self.evaluate_type_for_assignability(candidate);
        let probe = if evaluated == TypeId::ERROR {
            candidate
        } else {
            evaluated
        };

        if probe != candidate && !visited.insert(probe) {
            return false;
        }

        if let Some(members) = union_members(self.ctx.types, probe) {
            return members
                .iter()
                .all(|&m| self.candidate_rejects_empty_object(m, visited));
        }

        if let Some(members) = intersection_members(self.ctx.types, probe) {
            return members
                .iter()
                .any(|&m| self.candidate_rejects_empty_object(m, visited));
        }

        let shape_id = object_shape_id(self.ctx.types, probe)
            .or_else(|| object_with_index_shape_id(self.ctx.types, probe));

        if let Some(shape_id) = shape_id
            && self
                .ctx
                .types
                .object_shape(shape_id)
                .properties
                .iter()
                .any(|prop| !prop.optional)
        {
            return true;
        }

        if crate::query_boundaries::common::has_call_signatures(self.ctx.types, probe)
            || crate::query_boundaries::common::has_construct_signatures(self.ctx.types, probe)
        {
            return true;
        }

        false
    }

    fn is_deferred_generic_index_for_object(
        &self,
        index_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members
                .iter()
                .copied()
                .any(|member| self.is_deferred_generic_index_for_object(member, object_type));
        }

        if let Some(keyof_operand) = get_keyof_type(self.ctx.types, index_type) {
            return keyof_operand == object_type;
        }

        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_operand) = get_keyof_type(self.ctx.types, constraint)
        {
            return keyof_operand == object_type;
        }

        false
    }

    fn is_generic_index_key_assignable(&mut self, source_key: TypeId, target_key: TypeId) -> bool {
        if self.type_parameter_identities_match(source_key, target_key) {
            return true;
        }

        if crate::query_boundaries::common::type_param_info(self.ctx.types, source_key).is_some()
            && crate::query_boundaries::common::type_param_info(self.ctx.types, target_key)
                .is_some()
        {
            return self.type_param_constraint_chain_reaches(source_key, target_key);
        }

        self.is_assignable_to(source_key, target_key)
    }

    fn type_parameter_identities_match(&self, source: TypeId, target: TypeId) -> bool {
        source == target
            || self
                .ctx
                .definition_store
                .find_def_for_type(source)
                .zip(self.ctx.definition_store.find_def_for_type(target))
                .is_some_and(|(source_def, target_def)| source_def == target_def)
    }

    fn type_param_constraint_chain_reaches(&self, source: TypeId, target: TypeId) -> bool {
        let mut current = source;
        let mut seen = FxHashSet::default();

        while seen.insert(current) {
            if self.type_parameter_identities_match(current, target) {
                return true;
            }

            let Some(current_param) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, current)
            else {
                return false;
            };
            let Some(constraint) = current_param.constraint else {
                return false;
            };

            current = constraint;
        }

        false
    }

    fn collect_deferred_index_access_candidate_types(
        &mut self,
        object_type: TypeId,
        candidate_types: &mut Vec<TypeId>,
    ) {
        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, object_type)
            && let Some(constraint) = param_info.constraint
        {
            self.collect_deferred_index_access_candidate_types(constraint, candidate_types);
            return;
        }

        self.ensure_relation_input_ready(object_type);
        let evaluated = self.evaluate_type_for_assignability(object_type);
        if evaluated != object_type && evaluated != TypeId::ERROR {
            self.collect_deferred_index_access_candidate_types(evaluated, candidate_types);
            if !candidate_types.is_empty() {
                return;
            }
        }

        if let Some(members) = crate::query_boundaries::common::union_members(
            self.ctx.types,
            object_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::intersection_members(self.ctx.types, object_type)
        }) {
            for member in members.iter().copied() {
                self.collect_deferred_index_access_candidate_types(member, candidate_types);
            }
            return;
        }

        let shape_id = crate::query_boundaries::common::object_shape_id(
            self.ctx.types,
            object_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::object_with_index_shape_id(self.ctx.types, object_type)
        });

        if let Some(shape_id) = shape_id {
            let shape = self.ctx.types.object_shape(shape_id);
            candidate_types.extend(shape.properties.iter().map(|prop| {
                if prop.optional {
                    self.ctx.types.union2(prop.type_id, TypeId::UNDEFINED)
                } else {
                    prop.type_id
                }
            }));
        }

        let index_info = self.ctx.types.get_index_signatures(object_type);
        if let Some(string_index) = index_info.string_index {
            candidate_types.push(string_index.value_type);
        }
        if let Some(number_index) = index_info.number_index {
            candidate_types.push(number_index.value_type);
        }
    }

    /// Like `is_assignable_to`, but skips weak type checks (TS2559).
    ///
    /// This matches tsc's `isTypeAssignableTo` behavior, which does NOT
    /// include the weak type check. Used by the flow narrowing guard to
    /// avoid rejecting valid type-guard narrowing (e.g., instanceof).
    pub fn is_assignable_to_no_weak_checks(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let source = self.substitute_this_type_if_needed(source);
        let target = self.substitute_this_type_if_needed(target);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let overrides = CheckerOverrideProvider::new(self, None);
        crate::query_boundaries::assignability::is_assignable_no_weak_checks(
            &AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags: self.ctx.pack_relation_flags(),
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            },
            &overrides,
        )
    }

    /// Like `is_assignable_to`, but disables generic type parameter erasure.
    ///
    /// Used for implements/extends member type checking (TS2416) where tsc's
    /// `compareSignaturesRelated` does NOT erase target type parameters.
    /// A non-generic `(x: string) => string` is NOT assignable to a generic
    /// `<T>(x: T) => T` under this mode.
    pub fn is_assignable_to_no_erase_generics(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::NO_ERASE_GENERICS,
            "is_assignable_to_no_erase_generics",
        )
    }

    /// Like `is_assignable_to`, but forces the strict-function-types relation flag.
    pub fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES,
            "is_assignable_to_strict",
        )
    }

    /// Check assignability while forcing strict null checks in relation flags.
    ///
    /// This keeps the regular checker/solver assignability gateway (resolver,
    /// overrides, caching, and precondition setup) while pinning nullability
    /// semantics to strict mode for localized checks.
    pub fn is_assignable_to_strict_null(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        self.check_assignability_cached(
            source,
            target,
            crate::query_boundaries::assignability::RelationFlags::STRICT_NULL_CHECKS,
            "is_assignable_to_strict_null",
        )
    }

    /// Check assignability with the current `TypeEnvironment` but without
    /// consulting the checker's relation caches.
    ///
    /// Generic call/new inference uses this after instantiation to avoid stale
    /// relation answers while still going through the same input preparation as
    /// the normal assignability gateway.
    pub fn is_assignable_to_with_env(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        self.ensure_relation_inputs_ready(&[source, target]);
        let target = self.substitute_this_type_if_needed(target);

        if source != TypeId::NEVER
            && self.is_concrete_source_to_deferred_keyof_index_access(source, target)
        {
            return false;
        }

        {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let inputs = AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &*env,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
            };
            if let Some(result) = check_application_variance_assignability(&inputs) {
                return result;
            }
        }

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        let result = {
            let env = self.ctx.type_env.borrow();
            let flags = self.ctx.pack_relation_flags();
            let overrides = CheckerOverrideProvider::new(self, Some(&*env));
            let relation_result = is_assignable_with_overrides(
                &AssignabilityQueryInputs {
                    db: self.ctx.types,
                    resolver: &*env,
                    source,
                    target,
                    flags,
                    inheritance_graph: &self.ctx.inheritance_graph,
                    sound_mode: self.ctx.sound_mode(),
                },
                &overrides,
            );
            self.propagate_overflow_flags(
                relation_result.depth_exceeded,
                relation_result.iteration_exceeded,
            );
            relation_result.is_related()
        };

        if result
            && self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }

        if let Some(keyof_type) = get_keyof_type(self.ctx.types, target)
            && let Some(source_atom) = get_string_literal_value(self.ctx.types, source)
        {
            let source_str = self.ctx.types.resolve_atom(source_atom);
            let allowed_keys = get_allowed_keys(self.ctx.types, keyof_type);
            // Only reject when we could determine concrete keys. An empty set means
            // the inner type couldn't be resolved (e.g., ThisType, TypeParameter,
            // or Application). In that case, trust the solver's result.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&source_str) {
                return false;
            }
        }

        result
    }

    /// Check if `source` type is assignable to `target` type with bivariant function parameter checking.
    ///
    /// This is used for class method override checking, where methods are always bivariant
    /// (unlike function properties which are contravariant with strictFunctionTypes).
    ///
    /// Follows the same pattern as `is_assignable_to` but calls `is_assignable_to_bivariant_callback`
    /// which disables `strict_function_types` for the check.
    pub fn is_assignable_to_bivariant(&mut self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }
        // CRITICAL: Ensure all Ref types are resolved before assignability check.
        // This fixes intersection type assignability where `type AB = A & B` needs
        // A and B in type_env before we can check if a type is assignable to the intersection.
        self.ensure_relation_inputs_ready(&[source, target]);

        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);

        // Check relation cache for non-inference types
        // Construct RelationCacheKey with Lawyer-layer flags to prevent cache poisoning
        // Note: Use ORIGINAL types for cache key, not evaluated types
        let is_cacheable = is_relation_cacheable(self.ctx.types, source, target);

        // For bivariant checks, we strip the strict_function_types flag
        // so the cache key is distinct from regular assignability checks.
        let flags = self.ctx.pack_relation_flags()
            & !crate::query_boundaries::assignability::RelationFlags::STRICT_FUNCTION_TYPES;

        if is_cacheable {
            // Note: For assignability checks, we use AnyPropagationMode::All (0)
            // since the checker doesn't track depth like SubtypeChecker does
            let cache_key = assignability_cache_key(source, target, flags);

            if let Some(cached) = self.ctx.types.lookup_assignability_cache(cache_key) {
                return cached;
            }
        }

        let env = self.ctx.type_env.borrow();
        // Preserve existing behavior: bivariant path does not use checker overrides.
        let relation_result = is_assignable_bivariant_with_resolver(
            self.ctx.types,
            &*env,
            source,
            target,
            flags,
            &self.ctx.inheritance_graph,
            self.ctx.sound_mode(),
        );
        self.propagate_overflow_flags(
            relation_result.depth_exceeded,
            relation_result.iteration_exceeded,
        );
        let result = relation_result.is_related();

        // Cache the result for non-inference types
        // Use ORIGINAL types for cache key (not evaluated types)
        if is_cacheable {
            let cache_key = assignability_cache_key(source, target, flags);

            self.ctx.types.insert_assignability_cache(cache_key, result);
        }

        trace!(
            source = source.0,
            target = target.0,
            result,
            "is_assignable_to_bivariant"
        );
        result
    }

    /// Check if two types have any overlap (can ever be equal).
    ///
    /// Used for TS2367: "This condition will always return 'false'/'true' since
    /// the types 'X' and 'Y' have no overlap."
    ///
    /// Returns true if the types can potentially be equal, false if they can never
    /// have any common value.
    pub fn are_types_overlapping(&mut self, left: TypeId, right: TypeId) -> bool {
        // Ensure centralized relation preconditions before overlap check.
        self.ensure_relation_input_ready(left);
        self.ensure_relation_input_ready(right);

        let env = self.ctx.type_env.borrow();
        are_types_overlapping_with_env(
            self.ctx.types,
            &env,
            left,
            right,
            self.ctx.strict_null_checks(),
        )
    }
}
