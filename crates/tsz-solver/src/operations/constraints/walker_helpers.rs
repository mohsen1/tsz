//! Helper methods for the core constraint walker.

use crate::def::DefId;
use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::relations::variance::compute_type_param_variances_with_resolver_cached;
use crate::types::{
    MappedType, ObjectShape, ParamInfo, PropertyInfo, TupleElement, TypeData, TypeId, Variance,
};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Propagate `any` to inference placeholders that appear as **naked** type
    /// variables in `target`.
    ///
    /// tsc's `propagationType` mechanism calls `inferFromTypes(target, target)`
    /// with the source as the propagation type, but it only reaches placeholders
    /// that are directly visible as type-variable positions -- i.e. the target is
    /// itself a type parameter, or it is a union/intersection whose members are
    /// walked recursively. It does NOT walk into arrays, tuples, objects, index
    /// signatures, function shapes, or generic application arguments.
    ///
    /// Concretely:
    /// - `f<T>(x: T)` with `any` -> T = `any`               (direct naked T)
    /// - `f<T>(x: T | string)` with `any` -> T = `any`      (union member)
    /// - `f<T>(x: T[])` with `any` -> T = `unknown`         (array, not propagated)
    /// - `f<T>(x: { v: T })` with `any` -> T = `unknown`    (object, not propagated)
    /// - `f<T>(x: { [s: string]: T })` with `any` -> T = `unknown` (index sig, not propagated)
    /// - `f<T>(x: Promise<T>)` with `any` -> T = `unknown`  (object application, not propagated)
    /// - `f<T>(x: Awaited<T>)` with `any` -> T = `any`     (conditional alias, true/false branch)
    /// - `f<T>(x: A extends B ? T : C)` with `any` -> T = `any` (naked T in true/false branch)
    /// - `f<T>(x: T extends B ? C : D)` with `any` -> T = `unknown` (T only in check, not propagated)
    pub(super) fn propagate_type_to_placeholders(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        propagation_type: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, propagation_type, priority);
            return;
        }

        match self.interner.lookup(target) {
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                for &member in members.iter() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        member,
                        priority,
                    );
                }
            }
            Some(TypeData::Lazy(_)) | Some(TypeData::Application(_)) => {
                let resolved = self.checker.evaluate_type(target);
                if resolved != target {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        resolved,
                        priority,
                    );
                }
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                let true_type = cond.true_type;
                let false_type = cond.false_type;
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    true_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    false_type,
                    priority,
                );
            }
            _ => {}
        }
    }

    /// Constrain type arguments of two Applications with the same base type,
    /// respecting the variance of each type parameter position.
    pub(super) fn constrain_application_type_args(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        base: TypeId,
        source_args: &[TypeId],
        target_args: &[TypeId],
        priority: crate::types::InferencePriority,
    ) {
        let variances = self.compute_application_variances(base);
        for (i, (s_arg, t_arg)) in source_args.iter().zip(target_args.iter()).enumerate() {
            let variance = variances
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or(Variance::COVARIANT);
            if variance.is_contravariant() {
                let was_contra = ctx.in_contra_mode;
                ctx.in_contra_mode = !was_contra;
                self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
                ctx.in_contra_mode = was_contra;
            } else {
                self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
            }
        }
    }

    /// Compute the variances of each type parameter for a type application's base type.
    pub(super) fn compute_application_variances(
        &self,
        base: TypeId,
    ) -> Option<std::sync::Arc<[Variance]>> {
        let def_id = self.application_base_def_id_for_constraint(base)?;
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        compute_type_param_variances_with_resolver_cached(
            self.interner.as_type_database(),
            resolver,
            Some(self.interner),
            def_id,
        )
    }

    pub(super) fn application_bases_share_declaration(
        &self,
        source_base: TypeId,
        target_base: TypeId,
    ) -> bool {
        if source_base == target_base {
            return true;
        }
        let Some(source_def) = self.application_base_def_id_for_constraint(source_base) else {
            return false;
        };
        let Some(target_def) = self.application_base_def_id_for_constraint(target_base) else {
            return false;
        };
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        let source_def = resolver.canonical_def_id(source_def);
        let target_def = resolver.canonical_def_id(target_def);
        resolver.defs_are_equivalent(source_def, target_def)
    }

    fn application_base_def_id_for_constraint(&self, base: TypeId) -> Option<DefId> {
        if base.is_intrinsic() {
            return None;
        }
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => resolver.symbol_to_def_id(sym_ref),
            TypeData::UnresolvedTypeName(atom) => {
                let name = self.interner.resolve_atom(atom);
                resolver.resolve_unresolved_type_name(&name)
            }
            _ => None,
        }
    }

    /// Constrain source properties against target properties for two object
    /// shapes, propagating freshness from the source's `FRESH_LITERAL` flag.
    pub(super) fn constrain_object_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        s_shape: &ObjectShape,
        t_shape: &ObjectShape,
        priority: crate::types::InferencePriority,
    ) {
        let source_is_fresh = s_shape
            .flags
            .contains(crate::types::ObjectFlags::FRESH_LITERAL);
        self.constrain_properties(
            ctx,
            var_map,
            &s_shape.properties,
            &t_shape.properties,
            priority,
            source_is_fresh,
        );
    }

    /// If the target's last parameter is a rest parameter typed as a direct
    /// inference variable, collect the source's trailing parameters past the
    /// target's fixed arity into a tuple and add it as a `NakedTypeVariable`
    /// candidate for that variable.
    pub(super) fn infer_rest_param_tuple_candidate(
        &self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
    ) {
        let Some(t_last) = target_params.last() else {
            return;
        };
        if !t_last.rest {
            return;
        }
        let Some(&var) = var_map.get(&t_last.type_id) else {
            return;
        };
        let target_fixed_count = target_params.len().saturating_sub(1);
        if source_params.len() <= target_fixed_count {
            return;
        }
        let tuple_elements: Vec<TupleElement> = source_params[target_fixed_count..]
            .iter()
            .map(|p| TupleElement {
                type_id: if p.optional {
                    self.interner.union2(p.type_id, TypeId::UNDEFINED)
                } else {
                    p.type_id
                },
                name: p.name,
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let needs_regular_candidate = tuple_elements.iter().any(|elem| {
            elem.optional
                || elem.rest
                || self.rest_tuple_element_needs_regular_candidate(elem.type_id)
        });
        let source_tuple = self.interner.tuple(tuple_elements);
        if needs_regular_candidate {
            ctx.add_candidate(
                var,
                source_tuple,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        } else {
            ctx.add_contra_candidate(
                var,
                source_tuple,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }
    }

    fn rest_tuple_element_needs_regular_candidate(&self, ty: TypeId) -> bool {
        if crate::visitor::contains_type_parameters(self.interner.as_type_database(), ty)
            || crate::type_queries::contains_infer_types_db(self.interner.as_type_database(), ty)
        {
            return true;
        }

        matches!(
            self.interner.lookup(ty),
            Some(TypeData::Union(_) | TypeData::Intersection(_))
        )
    }

    /// For each source property, instantiate the mapped type's template by
    /// substituting the iteration variable with the property's key literal,
    /// then constrain the property's value type against that instantiated
    /// template.
    pub(super) fn constrain_template_against_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        properties: &[PropertyInfo],
        mapped: &MappedType,
        priority: crate::types::InferencePriority,
    ) {
        if var_map.is_empty() {
            return;
        }
        let iter_param_name = mapped.type_param.name;
        for prop in properties {
            let key_literal = crate::utils::literal_key_for_property_name(
                self.interner,
                prop.name,
                prop.is_string_named,
            );
            let subst = TypeSubstitution::single(iter_param_name, key_literal);
            let instantiated_template = instantiate_type(self.interner, mapped.template, &subst);
            self.constrain_types(ctx, var_map, prop.type_id, instantiated_template, priority);
        }
    }

    pub(super) fn remove_reverse_mapped_target_params(
        &self,
        var_map: &mut FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        target: TypeId,
    ) {
        let candidates: Vec<TypeId> = var_map.keys().copied().collect();
        for candidate in candidates {
            if candidate == target {
                var_map.remove(&candidate);
                continue;
            }

            let Some(var) = var_map.get(&candidate).copied() else {
                continue;
            };
            let mut probe_map = FxHashMap::default();
            probe_map.insert(candidate, var);
            let contains_placeholder =
                super::walker_guard_state::with_placeholder_visited(|visited| {
                    self.type_contains_placeholder(target, &probe_map, visited)
                });
            if contains_placeholder {
                var_map.remove(&candidate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::query_cache::QueryCache;
    use crate::def::DefId;
    use crate::intern::TypeInterner;
    use crate::relations::subtype::{TypeEnvironment, TypeResolver};
    use crate::types::{InferencePriority, PropertyInfo, SymbolRef, TypeParamInfo};

    struct ResolverBackedChecker<'a> {
        resolver: &'a dyn TypeResolver,
        assignable: bool,
    }

    impl AssignabilityChecker for ResolverBackedChecker<'_> {
        fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
            self.assignable
        }

        fn type_resolver(&self) -> Option<&dyn TypeResolver> {
            Some(self.resolver)
        }
    }

    struct PairEquivalentResolver {
        left: DefId,
        right: DefId,
    }

    impl TypeResolver for PairEquivalentResolver {
        fn resolve_ref(
            &self,
            _symbol: SymbolRef,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
            a == b || (a == self.left && b == self.right) || (a == self.right && b == self.left)
        }
    }

    #[test]
    fn compute_application_variances_reuses_query_cache() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let body = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            t_type,
        )]);
        let def_id = DefId(91_001);
        let base = interner.lazy(def_id);

        let mut env = TypeEnvironment::new();
        env.insert_def_with_params(def_id, body, vec![t_param]);
        let mut checker = ResolverBackedChecker {
            resolver: &env,
            assignable: true,
        };
        let evaluator = CallEvaluator::new(&cache, &mut checker);

        assert_eq!(cache.statistics().variance_cache_entries, 0);
        assert!(evaluator.compute_application_variances(base).is_some());
        assert_eq!(cache.statistics().variance_cache_entries, 1);
        assert!(evaluator.compute_application_variances(base).is_some());
        assert_eq!(cache.statistics().variance_cache_entries, 1);
    }

    #[test]
    fn equivalent_application_bases_constrain_type_args() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let source_def = DefId(143_570);
        let target_def = DefId(143_571);
        let resolver = PairEquivalentResolver {
            left: source_def,
            right: target_def,
        };
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: false,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);

        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let mut infer_ctx = InferenceContext::new(&interner);
        let var_t = infer_ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var_t);

        let source = interner.application(interner.lazy(source_def), vec![TypeId::STRING]);
        let target = interner.application(interner.lazy(target_def), vec![t_type]);
        evaluator.constrain_types(
            &mut infer_ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            infer_ctx
                .resolve_with_constraints(var_t)
                .expect("equivalent application bases must constrain the type arg"),
            TypeId::STRING
        );
    }

    #[test]
    fn unrelated_application_bases_do_not_constrain_type_args() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = PairEquivalentResolver {
            left: DefId(143_580),
            right: DefId(143_581),
        };
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: false,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);

        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let mut infer_ctx = InferenceContext::new(&interner);
        let var_t = infer_ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var_t);

        let source = interner.application(interner.lazy(DefId(143_582)), vec![TypeId::STRING]);
        let target = interner.application(interner.lazy(DefId(143_583)), vec![t_type]);
        evaluator.constrain_types(
            &mut infer_ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            infer_ctx
                .resolve_with_constraints(var_t)
                .expect("unconstrained inference var must still resolve (to unknown)"),
            TypeId::UNKNOWN
        );
    }
}
