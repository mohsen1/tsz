//! Core type constraint walker for generic type inference.
//!
//! Contains the main structural walker (`constrain_types` / `constrain_types_impl`)
//! that collects type constraints when inferring generic type parameters from
//! argument types.

include!("walker_large_methods/constrain_types_impl_11_4.rs");

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::{AssignabilityChecker, CallEvaluator, MAX_CONSTRAINT_RECURSION_DEPTH};
use crate::relations::variance::compute_type_param_variances_with_resolver;
use crate::types::{
    FunctionShape, IntrinsicKind, LiteralValue, MappedType, ObjectShape, ParamInfo, PropertyInfo,
    TemplateSpan, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate, Variance,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tracing::{debug, trace};

// Reusable scratch `FxHashSet<TypeId>` for the five `type_contains_placeholder`
// call-sites in this module. Each call previously allocated a fresh set;
// pooling shaves the allocator round-trip plus 2–4 grows. Mirrors the pool
// pattern from #4722 / #4790 / #4801 / #4805 / #4807.
thread_local! {
    static PLACEHOLDER_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> =
        const { RefCell::new(None) };
}

#[inline]
fn with_placeholder_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = PLACEHOLDER_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    PLACEHOLDER_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Structural walker to collect constraints: source <: target
    pub(crate) fn constrain_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        {
            let steps = self.constraint_step_count.get();
            if steps >= MAX_CONSTRAINT_STEPS {
                return;
            }
            self.constraint_step_count.set(steps + 1);
        }

        if !self.constraint_pairs.borrow_mut().insert((source, target)) {
            return;
        }

        // Check and increment recursion depth to prevent infinite loops
        {
            let depth = self.constraint_recursion_depth.get();
            if depth >= MAX_CONSTRAINT_RECURSION_DEPTH {
                // Safety limit reached - return to prevent infinite loop
                return;
            }
            self.constraint_recursion_depth.set(depth + 1);
        }

        // Perform the actual constraint collection
        self.constrain_types_impl(ctx, var_map, source, target, priority);

        // Decrement depth on return
        self.constraint_recursion_depth
            .set(self.constraint_recursion_depth.get() - 1);
    }

    /// Propagate `any` to inference placeholders that appear as **naked** type
    /// variables in `target`.
    ///
    /// tsc's `propagationType` mechanism calls `inferFromTypes(target, target)`
    /// with the source as the propagation type, but it only reaches placeholders
    /// that are directly visible as type-variable positions — i.e. the target is
    /// itself a type parameter, or it is a union/intersection whose members are
    /// walked recursively.  It does NOT walk into arrays, tuples, objects, index
    /// signatures, function shapes, or generic application arguments.
    ///
    /// Concretely:
    /// - `f<T>(x: T)` with `any` → T = `any`               (direct naked T)
    /// - `f<T>(x: T | string)` with `any` → T = `any`      (union member)
    /// - `f<T>(x: T[])` with `any` → T = `unknown`         (array, not propagated)
    /// - `f<T>(x: { v: T })` with `any` → T = `unknown`    (object, not propagated)
    /// - `f<T>(x: { [s: string]: T })` with `any` → T = `unknown` (index sig, not propagated)
    /// - `f<T>(x: Promise<T>)` with `any` → T = `unknown`  (object application, not propagated)
    /// - `f<T>(x: Awaited<T>)` with `any` → T = `any`     (conditional alias, true/false branch)
    /// - `f<T>(x: A extends B ? T : C)` with `any` → T = `any` (naked T in true/false branch)
    /// - `f<T>(x: T extends B ? C : D)` with `any` → T = `unknown` (T only in check, not propagated)
    pub(super) fn propagate_type_to_placeholders(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        propagation_type: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        // Direct placeholder: add the propagation type as a candidate.
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, propagation_type, priority);
            return;
        }

        match self.interner.lookup(target) {
            // Walk union/intersection members — naked T inside `T | string` must still
            // receive `any` as a candidate.
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
            // Resolve lazy aliases and evaluate generic type applications so that:
            // - `type MaybeT<T> = T | null` → expanded union, T is a reachable naked member
            // - `Awaited<T>` (Application wrapping a conditional) → expanded conditional,
            //   true/false branches are then walked to find naked T positions
            // Non-conditional applications (e.g. `Promise<T>`, `Array<T>`) expand to
            // object/array shapes which fall to `_ => {}`, preserving T = `unknown`.
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
            // Walk the true and false branches of a conditional type.
            // tsc propagates `any` into naked T positions in true/false branches
            // (e.g. `Awaited<T>` has `T` as its false branch so `f<T>(x: Awaited<T>)`
            // with `any` correctly infers T = `any`).
            // The check type and extends type are NOT walked: `T extends U ? ...`
            // gives T = `unknown` when T is only in check position.
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
            // Arrays, tuples, objects, index signatures, functions, callables,
            // mapped types, and all other structural positions are NOT walked.
            // tsc does not propagate `any` through nested structural shapes.
            _ => {}
        }
    }

    /// Constrain type arguments of two Applications with the same base type,
    /// respecting the variance of each type parameter position.
    ///
    /// For contravariant positions (e.g., T in `type Func<T> = (x: T) => void`),
    /// the source and target are swapped so that inference produces contra-candidates
    /// (resolved via intersection/most-specific) rather than covariant candidates.
    /// This matches tsc's `inferFromTypeArguments` which checks variance flags.
    pub(super) fn constrain_application_type_args(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        base: TypeId,
        source_args: &[TypeId],
        target_args: &[TypeId],
        priority: crate::types::InferencePriority,
    ) {
        // Try to compute variances for the base type's type parameters.
        let variances = self.compute_application_variances(base);
        for (i, (s_arg, t_arg)) in source_args.iter().zip(target_args.iter()).enumerate() {
            let variance = variances
                .as_ref()
                .and_then(|v| v.get(i).copied())
                .unwrap_or(Variance::COVARIANT);
            if variance.is_contravariant() {
                // Contravariant: swap source and target so the inference engine
                // sees the narrower type as source (lower bound on the target
                // placeholder). This causes the placeholder to pick the
                // intersection of candidates instead of the union.
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
        let def_id = match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => def_id,
            _ => return None,
        };
        // Use the checker's resolver which has type alias definitions,
        // falling back to the interner's resolver (which lacks them).
        let resolver = self
            .checker
            .type_resolver()
            .unwrap_or_else(|| self.interner.as_type_resolver());
        compute_type_param_variances_with_resolver(
            self.interner.as_type_database(),
            resolver,
            def_id,
        )
    }

    __tsz_split_walker_constrain_types_impl_11_4!();

    /// Constrain source properties against target properties for two object
    /// shapes, propagating freshness from the source's `FRESH_LITERAL` flag.
    ///
    /// All four `Object`/`ObjectWithIndex` arms of the main walker compute
    /// `source_is_fresh` from the same flag bit and feed it into
    /// [`Self::constrain_properties`]; this helper keeps that shared preamble
    /// in one place.
    fn constrain_object_properties(
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
    ///
    /// Example: source `(a: string, b: number) => R` vs target `(...args: A) => R`
    /// infers `A = [string, number]`.
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
            // Preserve the regular tuple candidate for optional/generic/union
            // rest inference paths that rely on the pre-existing covariant
            // candidate behavior.
            ctx.add_candidate(
                var,
                source_tuple,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        } else {
            // Simple fixed parameter lists should not also become covariant
            // candidates, or an array-literal argument can erase tuple arity.
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
    /// template. Used by both reverse-mapped inference (post-`keyof T`
    /// reconstruction) and simple mapped-type inference.
    fn constrain_template_against_properties(
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

    fn remove_reverse_mapped_target_params(
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
            let contains_placeholder = with_placeholder_visited(|visited| {
                self.type_contains_placeholder(target, &probe_map, visited)
            });
            if contains_placeholder {
                var_map.remove(&candidate);
            }
        }
    }
}
