//! Conditional type evaluation.
//!
//! Handles TypeScript's conditional types: `T extends U ? X : Y`
//! Including distributive conditional types over union types.

mod application_infer;
mod application_reduction;
mod array_infer;
mod callable_relation;
mod object_infer;
mod permissive;
mod phases;

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_generic_cached, instantiate_type, instantiate_type_with_infer,
};
use crate::operations::property::PropertyAccessResult;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallSignature, CallableShape, ConditionalType, ObjectShape, ObjectShapeId, ParamInfo,
    PropertyInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
};
use crate::visitor::{callable_shape_id, function_shape_id};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tracing::trace;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;
use super::infer_pattern::InferPatternVisited;
use crate::evaluation::result::TerminationKind;
use crate::type_queries::get_application_base;
use permissive::PermissiveFalseBranchProbe;
use phases::TailCallStep;
pub(in crate::evaluation::evaluate_rules::conditional) use phases::{
    BranchRelation, classify_branch_relation,
};

/// Maximum number of union members a distributive conditional type
/// (`Exclude`, `Extract`, `NonNullable`, and any user `T extends U ? X : Y`
/// distributed over a union) may expand before the solver bails with
/// `TypeId::ERROR`.
///
/// Distribution is linear in the member count — one conditional is evaluated
/// per member — so the dominant cost is bounded CPU/allocation rather than a
/// combinatorial blow-up. The previous value of `100` was far below the size of
/// real-world key spaces: filtering `keyof` over a generated API surface, a DOM
/// tag-name map, or an SDK enum routinely produces unions of 150–500 members.
/// When the cap was hit the conditional collapsed to `TypeId::ERROR`, which then
/// poisoned downstream `keyof`/relation decisions and silently dropped or
/// invented diagnostics (false negatives such as missing **TS2536** on indexed
/// access through such a key space).
///
/// Keep this at the conservative mapped-key floor. `DEFAULT_MAX_MAPPED_KEYS` is
/// target-split elsewhere, but a single cross-target conditional-distribution
/// budget avoids making native CI eagerly materialize React-sized surfaces that
/// the 250-budget path still defers.
pub(crate) const MAX_CONDITIONAL_DISTRIBUTION_SIZE: usize = 250;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Maximum depth for tail-recursive conditional evaluation.
    /// This allows patterns like `type Loop<T> = T extends [...infer R] ? Loop<R> : never`
    /// to work with up to 1000 recursive calls instead of being limited to `MAX_EVALUATE_DEPTH`.
    /// Exact parity with tsc's `tailCount` limit; canonical definition in [`crate::limits`].
    const MAX_TAIL_RECURSION_DEPTH: usize = crate::limits::MAX_TAIL_RECURSION_DEPTH;

    fn normalize_conditional_object_operand(&mut self, type_id: TypeId) -> TypeId {
        let (shape_id, with_index) = match self.interner().lookup(type_id) {
            Some(TypeData::Object(shape_id)) => (shape_id, false),
            Some(TypeData::ObjectWithIndex(shape_id)) => (shape_id, true),
            _ => return type_id,
        };

        let shape = self.interner().object_shape(shape_id);

        // Pre-scan: evaluate every property to prime the evaluator cache and check
        // whether any TypeId changed. Use |= (not .any()) so that every property is
        // evaluated unconditionally — short-circuiting would leave later properties
        // un-cached, defeating the guarantee that the update pass below is cache-only.
        // Deferring the clone until we know a change is needed avoids an O(P) allocation
        // on every conditional evaluation for the common no-change case.
        let str_val = shape
            .string_index
            .as_ref()
            .map(|idx| self.evaluate(idx.value_type));
        let num_val = shape
            .number_index
            .as_ref()
            .map(|idx| self.evaluate(idx.value_type));
        let sym_val = shape
            .symbol_index
            .as_ref()
            .map(|idx| self.evaluate(idx.value_type));
        let mut any_changed = str_val
            .zip(shape.string_index.as_ref())
            .is_some_and(|(v, idx)| v != idx.value_type)
            || num_val
                .zip(shape.number_index.as_ref())
                .is_some_and(|(v, idx)| v != idx.value_type)
            || sym_val
                .zip(shape.symbol_index.as_ref())
                .is_some_and(|(v, idx)| v != idx.value_type);
        for prop in shape.properties.iter() {
            let rt = self.evaluate(prop.type_id);
            let wt = self.evaluate(prop.write_type);
            any_changed |= rt != prop.type_id || wt != prop.write_type;
        }

        if !any_changed {
            return type_id;
        }

        // At least one property changed: clone and apply. Every evaluate() call below
        // is a guaranteed cache hit because the pre-scan above evaluated every property.
        let flags = shape.flags;
        let symbol = shape.symbol;
        let mut properties = shape.properties.clone();
        for prop in &mut properties {
            prop.type_id = self.evaluate(prop.type_id);
            prop.write_type = self.evaluate(prop.write_type);
        }

        let mut string_index = shape.string_index;
        if let (Some(index), Some(v)) = (string_index.as_mut(), str_val) {
            index.value_type = v;
        }

        let mut number_index = shape.number_index;
        if let (Some(index), Some(v)) = (number_index.as_mut(), num_val) {
            index.value_type = v;
        }

        let mut symbol_index = shape.symbol_index;
        if let (Some(index), Some(v)) = (symbol_index.as_mut(), sym_val) {
            index.value_type = v;
        }

        let result = if with_index {
            self.interner().object_with_index(ObjectShape {
                flags,
                properties,
                string_index,
                number_index,
                symbol_index,
                symbol,
            })
        } else {
            self.interner()
                .object_with_flags_and_symbol(properties, flags, symbol)
        };

        // Re-interning the object under evaluated property types produces a
        // fresh `TypeId` that has no provenance of its own. Carry the source's
        // merged-intersection origin (and its display alias) across so an
        // eagerly merged object intersection stays recoverable as an
        // intersection after its members reduce.
        //
        // Without this, `{ a: 1 } & { a: 1 | number }` — merged at intern time
        // into `{ a: 1 & (1 | number) }` with a recorded origin — loses that
        // origin here once the property intersection evaluates to `1`, leaving
        // a bare object that the conditional extends-clause identity guard
        // (`intersection_member_set`) can no longer distinguish from a plain
        // `{ a: 1 }`. tsc keeps the written `IntersectionType` distinct from
        // the flattened object under `isTypeIdenticalTo`, so the higher-order
        // `Equal<A & M, A>` probe must answer `false` (#16095).
        self.propagate_merged_object_operand_provenance(type_id, result);

        result
    }

    /// Carry merged-intersection provenance from a conditional object operand to
    /// the re-interned, property-evaluated `result`. First-write-wins on the
    /// interner side, so this is a no-op when `result` already carries its own
    /// origin (or when `result` is unchanged from `source`).
    fn propagate_merged_object_operand_provenance(&mut self, source: TypeId, result: TypeId) {
        if source == result {
            return;
        }
        let Some(origin) = self.interner().get_merged_intersection_origin(source) else {
            return;
        };
        // Never stamp an intersection origin onto a `result` that is itself one
        // of the origin's own members. Once a property intersection reduces, the
        // re-interned object can hash-cons to a plain member (`{ a: 1 } & { a: 1
        // | number }` → `{ a: 1 }`), and that member `TypeId` is shared with
        // every unrelated `{ a: 1 }` in the program. Giving it a phantom
        // intersection provenance would mis-elaborate diagnostics on code that
        // never wrote an intersection. This mirrors the guard in
        // `normalize_intersection` that records an origin only for a genuinely
        // distinct merged object.
        if let Some(TypeData::Intersection(list)) = self.interner().lookup(origin)
            && self.interner().type_list(list).contains(&result)
        {
            return;
        }
        self.interner()
            .store_merged_intersection_origin(result, origin);
        if let Some(alias) = self.interner().get_display_alias(source) {
            self.interner().store_display_alias(result, alias);
        }
    }

    fn conditional_subtype_checker(&self) -> SubtypeChecker<'a, R> {
        let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access();
        checker.exact_optional_property_types = self.exact_optional_property_types();
        if let Some(query_db) = self.query_db() {
            checker = checker.with_query_db(query_db);
        }
        if let Some(session) = self.evaluation_session() {
            checker = checker.with_evaluation_session(session);
        }
        checker
    }

    /// tsc's permissive-instantiation false-branch gate (`getConditionalType`).
    ///
    /// When the check type is still generic, a failed relation against the
    /// extends type is only *definitive* if it also fails under the permissive
    /// instantiation — every named type parameter replaced by `any`
    /// (tsc's `wildcardType`). `Exclude<keyof Params, never>` resolves its
    /// false branch this way (`keyof any` is not assignable to `never`
    /// regardless of `Params`), while genuinely indeterminate relations stay
    /// deferred. Returns `true` when the false branch is definitive.
    ///
    /// ## Wildcard fidelity guard
    /// tsc's `wildcardType` is *not* plain `any`: it is preserved symbolically
    /// inside instantiable index operators (`keyof`, indexed access,
    /// string-mapping, conditional). Substituting concrete `any` for a free
    /// parameter only reproduces the permissive judgment when the resulting
    /// forms fully concretize. When the wildcard substitution leaves a permissive
    /// form that is *still* a deferred generic-marker type — e.g.
    /// `keyof <conditional>` whose operand did not reduce under `any` because it
    /// is a circular/conditional constraint — the `any` form no longer relates
    /// the way tsc's symbolic `wildcardType` would, so a relation failure is not
    /// a sound definitive-false witness and the conditional must stay deferred.
    /// This is exactly the react-redux-shaped circularly-constrained
    /// `keyof DecorationTargetProps extends keyof Shared<…>` in `Matching` /
    /// `Shared` mapped conditionals, which tsc keeps deferred. Concrete generic
    /// *references* (`Wrapped<T>` → `Wrapped<any>`, a fully resolved object that
    /// genuinely lacks `then`) and operators that collapse to a concrete key
    /// union (`keyof Params` → `string | number | symbol`) concretize
    /// faithfully, so their definitive false stands.
    fn permissive_false_branch_is_definitive(
        &mut self,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> bool {
        let cache_key = self.permissive_false_branch_key(check_type, extends_type);
        if let Some(cached) = self.cached_permissive_false_branch(&cache_key) {
            return cached;
        }
        let shared_cache_allowed = self.permissive_false_branch_shared_cache_allowed();
        if shared_cache_allowed
            && let Some(cached) = self.interner().lookup_permissive_false_branch_verdict(
                check_type,
                extends_type,
                self.no_unchecked_indexed_access(),
                self.exact_optional_property_types(),
            )
        {
            self.cache_permissive_false_branch(cache_key, cached);
            return cached;
        }

        let probe_state = self.evaluation_probe_state();
        let probe = self.permissive_false_branch_is_definitive_uncached(check_type, extends_type);
        if probe.cacheable
            && self.evaluation_probe_state_is_unchanged(probe_state)
            && self.request_state_is_depth_agnostic_cache_stable()
        {
            self.cache_permissive_false_branch(cache_key, probe.definitive_false);
            if shared_cache_allowed
                && let Some((permissive_check, permissive_extends)) = probe.permissive_pair
                && self
                    .interner()
                    .lookup_conditional_branch_verdict(
                        permissive_check,
                        permissive_extends,
                        self.no_unchecked_indexed_access(),
                        self.exact_optional_property_types(),
                    )
                    .is_some()
            {
                self.interner().insert_permissive_false_branch_verdict(
                    check_type,
                    extends_type,
                    self.no_unchecked_indexed_access(),
                    self.exact_optional_property_types(),
                    probe.definitive_false,
                );
            }
        }
        probe.definitive_false
    }

    fn permissive_false_branch_is_definitive_uncached(
        &mut self,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> PermissiveFalseBranchProbe {
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
        let mut params = self.extract_type_params_from_type(check_type).to_vec();
        for &param in self.extract_type_params_from_type(extends_type).iter() {
            if !params.iter().any(|existing| existing.name == param.name) {
                params.push(param);
            }
        }
        if params.is_empty() {
            // No named parameters to widen unless the check is an unresolved
            // operator. Do not use the broader generic-marker predicate: string
            // mappings like `Lowercase<T>` are evaluated through constraints.
            if matches!(
                self.interner().lookup(check_type),
                Some(TypeData::IndexAccess(_, _) | TypeData::KeyOf(_) | TypeData::Conditional(_))
            ) {
                return PermissiveFalseBranchProbe::unshared(false);
            }
            return PermissiveFalseBranchProbe::unshared(true);
        }
        let mut substitution = TypeSubstitution::new();
        for param in &params {
            substitution.insert(param.name, TypeId::ANY);
        }
        let permissive_check =
            self.evaluate(instantiate_type(self.interner(), check_type, &substitution));
        let permissive_extends = self.evaluate(instantiate_type(
            self.interner(),
            extends_type,
            &substitution,
        ));
        // After substitution, a permissive form that is *still* a deferred
        // index-like generic marker (e.g. `keyof <conditional>` that did not
        // reduce under `any`) means the `any` substitution failed to concretize
        // the relation. tsc's symbolic `wildcardType` would relate permissively
        // there; our collapsed `any` form does not, so a failure here is not a
        // sound definitive false — defer.
        if crate::type_queries::is_generic_conditional_check_type(self.interner(), permissive_check)
            || crate::type_queries::is_generic_conditional_check_type(
                self.interner(),
                permissive_extends,
            )
        {
            return PermissiveFalseBranchProbe::unshared(false);
        }
        // Only a definitive `Fails` makes the false branch definitive. `Holds`
        // relates under the permissive form, and `Undetermined` consumed an
        // unregistered `Lazy` body — both keep the conditional deferred (#14238).
        match self.conditional_subtype_relation(permissive_check, permissive_extends) {
            BranchRelation::Fails => PermissiveFalseBranchProbe::with_permissive_pair(
                true,
                permissive_check,
                permissive_extends,
            ),
            BranchRelation::Holds => PermissiveFalseBranchProbe::with_permissive_pair(
                false,
                permissive_check,
                permissive_extends,
            ),
            BranchRelation::Undetermined => PermissiveFalseBranchProbe::uncacheable(false),
        }
    }

    fn generic_extends_can_use_permissive_false_branch(&self, extends_type: TypeId) -> bool {
        crate::visitors::visitor_predicates::is_tuple_type(self.interner(), extends_type)
    }

    /// Whether `check_type` is a *non-generic* `IndexAccess` whose object is a
    /// reference to a user interface the active resolver cannot resolve yet.
    ///
    /// Such a check type — e.g. `Atom<unknown>['read']` while the user interface
    /// `Atom`'s `DefId -> TypeId` is not yet registered in this evaluation
    /// context (the resolution-order window of #13980) — survives evaluation
    /// unreduced. Matching an infer pattern (`Parameters<…>`'s
    /// `(...args: infer P) => any`) against it then fails, and because the check
    /// type carries no type parameters none of the generic-deferral guards apply,
    /// so the conditional would collapse to its `never` false branch. That
    /// `false` is schedule-dependent: the identical conditional matches its true
    /// branch once the interface is registered (the consuming call/relation path
    /// readies it). So the caller defers instead, the infer-path analogue of the
    /// existing `UnresolvedTypeName` / unresolved-`Application` deferrals (#14164).
    ///
    /// The gate is deliberately narrow — a *concrete* `IndexAccess` over a single
    /// unresolvable interface reference — so it never touches a generic indexed
    /// access (handled by the generic guards) nor a recursive `Application` /
    /// `Conditional` check type whose definitive branch is correct.
    fn index_access_blocks_on_unresolved_interface(&self, check_type: TypeId) -> bool {
        let Some(TypeData::IndexAccess(object, _)) = self.interner().lookup(check_type) else {
            return false;
        };
        if crate::visitor::contains_free_type_parameters(self.interner(), object) {
            return false;
        }
        // The interface reference is the index object itself (`I['k']`) or the
        // base of an application of it (`I<…>['k']`).
        let interface_ref = get_application_base(self.interner(), object).unwrap_or(object);
        let Some(def_id) = crate::visitor::lazy_def_id(self.interner(), interface_ref) else {
            return false;
        };
        self.resolver()
            .resolve_lazy(def_id, self.interner())
            .is_none()
    }

    /// Helper to recursively evaluate a conditional while respecting
    /// recursion-identity containment.
    pub(crate) fn recurse_conditional(&mut self, conditional: TypeId) -> TypeId {
        self.with_meta_rereduce_recursion_identity(conditional, conditional, |evaluator| {
            evaluator.evaluate(conditional)
        })
    }

    /// Evaluate a conditional type: T extends U ? X : Y
    ///
    /// Algorithm:
    /// 1. If `check_type` is a union and the conditional is distributive, distribute
    /// 2. Otherwise, check if `check_type` <: `extends_type`
    /// 3. If true -> return `true_type`
    /// 4. If false (disjoint) -> return `false_type`
    /// 5. If ambiguous (unresolved type param) -> return deferred conditional
    ///
    /// ## Tail-Recursion Elimination
    /// If the chosen branch (true/false) evaluates to another `ConditionalType`,
    /// we immediately evaluate it in the current stack frame instead of recursing.
    /// This allows tail-recursive patterns to work with up to `MAX_TAIL_RECURSION_DEPTH`
    /// iterations instead of being limited by `MAX_EVALUATE_DEPTH`.
    pub fn evaluate_conditional(&mut self, initial_cond: &ConditionalType) -> TypeId {
        // Setup loop state for tail-recursion elimination
        let mut current_cond_type = self.interner().conditional(*initial_cond);
        let mut current_cond = *initial_cond;
        let mut tail_recursion_count = 0;
        // PERF: Pre-allocate bindings and visited sets outside the tail-recursion
        // loop so their capacity is preserved across iterations.
        let mut loop_bindings: FxHashMap<Atom, TypeId> = FxHashMap::default();
        let mut loop_visited = InferPatternVisited::default();
        let mut tail_application_branch: Option<TypeId> = None;
        // Cycle detection for the tail-recursion loop.
        // Tracks (check_type, extends_type) pairs seen during tail calls.
        // When the same pair is encountered again, the conditional is cyclically
        // self-referential (e.g., the true/false branch evaluates back to the
        // same conditional). Without this, libraries like ts-toolbelt that have
        // deeply nested conditional types can cause infinite loops.
        let mut tail_seen: FxHashSet<(TypeId, TypeId, TypeId, TypeId)> = FxHashSet::default();
        let mut tail_identity_seen: Vec<TypeId> = Vec::new();

        loop {
            // Clear any apparent branch signal from the previous iteration so stale
            // signals don't leak into the outer evaluate_application.
            self.apparent_conditional_branch = None;

            // When tail recursion reaches the limit, the type didn't converge.
            // Flag TS2589 and return ERROR to prevent stack overflow.
            // This matches tsc's tail recursion limit of 1000 (instantiationCount).
            if tail_recursion_count >= Self::MAX_TAIL_RECURSION_DEPTH {
                self.mark_depth_exceeded_for_request();
                return TypeId::ERROR;
            }

            let cond = &current_cond;
            // Cycle detection: if we've seen this exact conditional state before,
            // the tail-recursion loop is cycling. Return ERROR to break the loop.
            if tail_recursion_count > 0
                && !tail_seen.insert((
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                ))
            {
                self.mark_depth_exceeded_for_request();
                return TypeId::ERROR;
            }

            if tail_recursion_count > 0
                && self.meta_rereduce_recursion_identity_would_exceed_with_seen(
                    current_cond_type,
                    &tail_identity_seen,
                )
            {
                self.record_request_limit_event(TerminationKind::IterationExceeded);
                return current_cond_type;
            }
            tail_identity_seen.push(current_cond_type);

            // Pre-evaluation Application-level infer matching.
            // When both check and extends are Applications (e.g., Promise<string> vs
            // Promise<infer U>), match type arguments directly before expanding.
            // After evaluation, Application types become structural Object/Callable types,
            // which may fail structural infer matching for complex interfaces like Promise.
            if let Some(result) = self.try_application_infer_match(cond) {
                return result;
            }

            // If the check side is already a deferred conditional over generic
            // inputs and the extends side has no infer pattern to bind, the
            // outer conditional is also indeterminate. Defer before evaluating
            // the check side; otherwise recursive helper aliases can expand
            // nested generic conditionals before any concrete instantiation is
            // available.
            if !self.type_contains_infer(cond.extends_type)
                && matches!(
                    self.interner().lookup(cond.check_type),
                    Some(TypeData::Conditional(_))
                )
                && crate::visitor::contains_type_parameters(self.interner(), cond.check_type)
            {
                return self.interner().conditional(*cond);
            }

            let ops = self.resolve_operands(cond);
            let check_type = ops.check_type;
            let extends_type = ops.extends_type;
            let extends_has_infer = ops.extends_has_infer;
            let extends_has_type_params = ops.extends_has_type_params;

            if cond.is_distributive && check_type == TypeId::NEVER {
                return TypeId::NEVER;
            }

            if check_type == TypeId::ANY {
                // For `any extends X ? T : F`, return union of both branches.
                // When X contains infer patterns, perform infer pattern matching
                // so the infer variables get bound to `any` and properly substituted.
                // e.g., `any extends infer U ? U : never` → union(any, never) → any
                if extends_has_infer {
                    let mut bindings = FxHashMap::default();
                    let mut visited = InferPatternVisited::default();
                    let mut checker = self.conditional_subtype_checker();
                    checker.allow_bivariant_rest = true;
                    self.match_infer_pattern(
                        check_type,
                        extends_type,
                        &mut bindings,
                        &mut visited,
                        &mut checker,
                    );
                    let true_sub = self.substitute_infer(cond.true_type, &bindings);
                    let false_sub = self.substitute_infer(cond.false_type, &bindings);
                    let true_eval = self.evaluate(true_sub);
                    let false_eval = self.evaluate(false_sub);
                    return self.interner().union2(true_eval, false_eval);
                }
                let true_eval = self.evaluate(cond.true_type);
                let false_eval = self.evaluate(cond.false_type);
                return self.interner().union2(true_eval, false_eval);
            }

            // Step 1: Check for distributivity
            // Only distribute for naked type parameters (recorded at lowering time).
            if cond.is_distributive
                && let Some(TypeData::Union(members)) = self.interner().lookup(check_type)
            {
                let members = self.interner().type_list(members);
                return self.distribute_conditional(
                    members.as_ref(),
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                );
            }

            if let Some(TypeData::Infer(info)) = self.interner().lookup(extends_type) {
                // A bare `infer X` extends clause always matches, so tsc takes the
                // true branch with `X` bound to the check type. Normal evaluation
                // defers when the check type is still a free type parameter (the
                // conditional has no concrete input yet). During the TS2589
                // depth-detection pass the alias body is evaluated with its type
                // parameters left free, so deferring here hides unconditionally
                // recursive aliases like `type A<T> = T extends infer X ? A<X & B>
                // : never` from the recursion guard. In that pass, bind `X` to the
                // free type parameter and follow the true branch so the guard can
                // observe the re-applied alias and surface TS2589.
                let check_is_unresolved_param = matches!(
                    self.interner().lookup(check_type),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                );
                let drive_recursion_for_depth_check = self.is_depth_detection_pass()
                    && matches!(
                        self.interner().lookup(check_type),
                        Some(TypeData::TypeParameter(_))
                    );
                if check_is_unresolved_param && !drive_recursion_for_depth_check {
                    return self.interner().conditional(*cond);
                }

                if check_type == TypeId::ANY {
                    let subst = TypeSubstitution::single(info.name, check_type);
                    let true_eval = self.evaluate(instantiate_type_with_infer(
                        self.interner(),
                        cond.true_type,
                        &subst,
                    ));
                    let false_eval = self.evaluate(instantiate_type_with_infer(
                        self.interner(),
                        cond.false_type,
                        &subst,
                    ));
                    return self.interner().union2(true_eval, false_eval);
                }

                let mut subst = TypeSubstitution::single(info.name, check_type);
                let mut inferred = check_type;
                if let Some(constraint) = info.constraint {
                    let mut checker = self.conditional_subtype_checker();
                    checker.allow_bivariant_rest = true;
                    let Some(filtered) =
                        self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                    else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                }

                subst.insert(info.name, inferred);

                let true_inst =
                    instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
                return self.evaluate_preserving_intersection_branch_alias(true_inst);
            }

            // `never` is the bottom type, assignable to every type, so a
            // non-distributive `never extends T ? X : Y` always selects the true
            // branch (the distributive case returned `never` above; a bare `infer`
            // extends was handled above and binds the variable to the check type
            // itself). When `T` is a structural pattern that contains `infer`
            // variables, matching the empty `never` source supplies no candidates,
            // so tsc resolves each `infer` to its default of `unknown`
            // (`never extends (a: infer P) => void ? P : …` → `unknown`). Without
            // this, an accumulator-style recursion driven by such a conditional
            // (the `UnionToTuple`/`LastOf` base case `UnionToIntersection<never>`)
            // never reaches its base case and trips a spurious TS2589.
            if check_type == TypeId::NEVER {
                let mut bindings: FxHashMap<Atom, TypeId> = FxHashMap::default();
                if extends_has_infer {
                    self.fill_unbound_infer_defaults(extends_type, TypeId::UNKNOWN, &mut bindings);
                }
                // `substitute_infer` is a no-op when `bindings` is empty (the
                // no-`infer` case), so this single path covers both.
                let true_inst = self.substitute_infer(cond.true_type, &bindings);
                return self.evaluate(true_inst);
            }

            let extends_unwrapped = match self.interner().lookup(extends_type) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => extends_type,
            };
            // Only strip a `ReadonlyType` wrapper from the source when the target is
            // also a readonly-array shape (`readonly T[]` / `ReadonlyArray<T>`).
            // Against a mutable array target the wrapper carries the variance signal
            // that direct assignment enforces via TS4104; keeping it makes the array
            // fast path's `extract_array_element` reject the source so evaluation
            // falls through to the strict subtype check (which returns false), instead
            // of silently treating the readonly source as mutable (issue #9743).
            let target_accepts_readonly_source = self
                .application_base_name_is_readonly_array(extends_type)
                || self.application_base_name_is_readonly_array(cond.extends_type);
            let check_unwrapped = match self.interner().lookup(check_type) {
                Some(TypeData::ReadonlyType(inner)) if target_accepts_readonly_source => inner,
                _ => check_type,
            };

            if extends_has_infer {
                // A check-side `this` forces deferral only when it is a *free*
                // contextual `this`; a *bound* `this` enclosed in a concrete
                // object/constructor instance (e.g. `InstanceType<typeof C>`)
                // is determined and must be evaluated, else fluent
                // `this`-returning classes get spurious TS2322/TS2345 (Kysely
                // `AlterColumnBuilder`). See `resolved_check_type_binds_this`.
                let check_has_free_this =
                    crate::contains_this_type(self.interner(), cond.check_type)
                        && !self.resolved_check_type_binds_this(check_type);
                if self.generic_tuple_infer_defer_required(cond.check_type, extends_unwrapped)
                    || check_has_free_this
                {
                    return self.interner().conditional(*cond);
                }
            }

            // Concrete-element fast paths run only when the extends shape contains no
            // free infer variables or type parameters; otherwise full structural relation
            // is required.
            let extends_is_concrete = !extends_has_infer && !extends_has_type_params;

            // PERF: Single lookup for array/tuple extends patterns with infer
            match self.interner().lookup(extends_unwrapped) {
                Some(TypeData::Array(ext_elem)) => {
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(ext_elem) {
                        return self.eval_conditional_array_infer(cond, check_unwrapped, info);
                    }
                    if extends_is_concrete
                        && let Some(result) = self.eval_conditional_array_concrete(
                            cond,
                            check_unwrapped,
                            ext_elem,
                            false,
                        )
                    {
                        return result;
                    }
                }
                Some(TypeData::Application(app_id)) => {
                    if let Some(info) = self.application_array_infer_pattern(app_id) {
                        return self.eval_conditional_array_infer(cond, check_unwrapped, info);
                    }
                }
                Some(TypeData::Tuple(extends_elements)) => {
                    let extends_elements = self.interner().tuple_list(extends_elements);
                    if extends_elements.len() == 1
                        && !extends_elements[0].rest
                        && let Some(TypeData::Infer(info)) =
                            self.interner().lookup(extends_elements[0].type_id)
                    {
                        return self.eval_conditional_tuple_infer(
                            cond,
                            check_unwrapped,
                            &extends_elements[0],
                            info,
                        );
                    }
                }
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    if let Some(result) =
                        self.eval_conditional_object_infer(cond, check_unwrapped, shape_id)
                    {
                        return result;
                    }
                    // Evaluated Array<X> becomes ObjectWithIndex. Use direct element comparison
                    // to avoid the expensive structural check that can fail due to cycle detection
                    // inside Array's method-signature conditional types.
                    if extends_is_concrete {
                        let allow_readonly =
                            self.application_base_name_is_readonly_array(cond.extends_type);
                        if let Some(target_elem) =
                            self.expanded_array_object_element(shape_id, allow_readonly)
                            && let Some(result) = self.eval_conditional_array_concrete(
                                cond,
                                check_unwrapped,
                                target_elem,
                                allow_readonly,
                            )
                        {
                            return result;
                        }
                    }
                }
                _ => {}
            }

            let raw_extends_unwrapped = match self.interner().lookup(cond.extends_type) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => cond.extends_type,
            };
            if raw_extends_unwrapped != extends_unwrapped
                && let Some(TypeData::Application(app_id)) =
                    self.interner().lookup(raw_extends_unwrapped)
            {
                if let Some(info) = self.application_array_infer_pattern(app_id) {
                    return self.eval_conditional_array_infer(cond, check_unwrapped, info);
                }
                // Fires when the raw extends is Application(Array, [X]) but the evaluated
                // form changed (e.g., to ObjectWithIndex). Use direct element comparison.
                if extends_is_concrete {
                    let allow_readonly =
                        self.application_base_name_is_readonly_array(raw_extends_unwrapped);
                    if let Some(target_elem) =
                        self.application_array_concrete_element(app_id, allow_readonly)
                        && let Some(result) = self.eval_conditional_array_concrete(
                            cond,
                            check_unwrapped,
                            target_elem,
                            allow_readonly,
                        )
                    {
                        return result;
                    }
                }
            }

            // Step 2: Check for naked type parameter
            if let Some(TypeData::TypeParameter(param)) = self.interner().lookup(check_type) {
                // Simplification: T extends never ? X : Y → Y
                // A type parameter T cannot extend `never` (only `never` extends `never`),
                // so the conditional always takes the false branch.
                if extends_type == TypeId::NEVER {
                    return self.evaluate(cond.false_type);
                }

                if cond.is_distributive
                    && check_type == extends_type
                    && cond.true_type == cond.check_type
                    && cond.false_type == TypeId::NEVER
                {
                    return check_type;
                }

                if !cond.is_distributive && check_type == extends_type {
                    return self.evaluate_preserving_intersection_branch_alias(cond.true_type);
                }

                // If extends_type contains infer patterns and the type parameter has a constraint,
                // try to infer from the constraint. This handles cases like:
                // R extends Reducer<infer S, any> ? S : never
                // where R is constrained to Reducer<any, any>
                if !cond.is_distributive
                    && extends_has_infer
                    && let Some(constraint) = param.constraint
                {
                    let mut checker = self.conditional_subtype_checker();
                    checker.allow_bivariant_rest = true;
                    let mut bindings = FxHashMap::default();
                    let mut visited = InferPatternVisited::default();
                    if self.match_infer_pattern(
                        constraint,
                        extends_type,
                        &mut bindings,
                        &mut visited,
                        &mut checker,
                    ) {
                        let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                        return self
                            .evaluate_preserving_intersection_branch_alias(substituted_true);
                    }
                }
                // When the check type is a type parameter, tsc keeps the conditional
                // deferred — it does NOT eagerly resolve based on the constraint.
                // Even if T's constraint satisfies extends_type (e.g., T extends string,
                // checking T extends string ? X : Y), the conditional stays deferred
                // because T could be instantiated with different subtypes of its constraint.
                //
                // The subtype checker handles source-position usage via
                // `conditional_branches_subtype` which computes the constraint on demand.
                // Target-position usage is handled via `subtype_of_conditional_target`
                // which also uses the constraint approach.
                //
                // Type parameter hasn't been substituted - defer evaluation.
                // Use evaluated check/extends types so the deferred conditional has
                // resolved TypeParameter references (not Lazy(DefId) wrappers).
                // This is critical for the subtype checker's get_conditional_constraint
                // which needs to recognize TypeParameter check_types via is_check_type_param.
                // Also evaluate true/false types to resolve Lazy alias references.
                //
                // EXCEPTION: When the raw extends_type is an Application containing infer
                // patterns (e.g., `Synthetic<T, infer V>`), preserve the raw form.
                // Evaluation would expand the Application into a structural Object, destroying
                // the Application structure that `try_application_infer_match` needs when
                // this deferred conditional is later instantiated with concrete type args
                // and re-evaluated.
                let true_type = self.evaluate(cond.true_type);
                let false_type = self.evaluate(cond.false_type);
                // Preserve the raw extends_type when it's an Application containing infer.
                // Evaluating an Application like `Synthetic<T, infer V>` can collapse it
                // to a structural Object (e.g., empty `{}`), losing the infer pattern.
                // When the deferred conditional is later instantiated, the Application form
                // is needed by `is_conditional_with_application_infer` and
                // `try_application_infer_match` to perform declaration-level infer matching.
                let deferred_extends = if matches!(
                    self.interner().lookup(cond.extends_type),
                    Some(TypeData::Application(_))
                ) && self.type_contains_infer(cond.extends_type)
                {
                    cond.extends_type
                } else {
                    extends_type
                };
                return self.interner().conditional(ConditionalType {
                    check_type,
                    extends_type: deferred_extends,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                });
            }

            // Step 2a: Identity simplification for any type (not just type params).
            // If check_type == extends_type, the conditional trivially takes the true branch,
            // regardless of what the raw check type contains.
            //
            // This must run before compound generic deferral: patterns like
            // `T["length"] extends N ? 1 : 0` can evaluate to concrete literals after
            // instantiation (`2 extends 2`) even though the raw check type is still an
            // indexed access containing type parameters.
            //
            // However, we must NOT take this shortcut when the *raw* (unevaluated)
            // extends_type contains `infer` patterns. In that case, the true branch
            // references infer type variables that must be bound via pattern matching
            // (Step 3). Taking the shortcut would return unbound infer types.
            // e.g., `Synthetic<number,number> extends Synthetic<T, infer V> ? V : never`
            //   Both sides evaluate to the same empty object, but V must be bound to number.
            if check_type == extends_type
                && !self.type_contains_infer(cond.extends_type)
                && !self.type_is_compound_generic(cond.extends_type)
            {
                if let Some(TypeData::Conditional(next_cond_id)) =
                    self.interner().lookup(cond.true_type)
                {
                    current_cond_type = cond.true_type;
                    current_cond = self.interner().get_conditional(next_cond_id);
                    tail_recursion_count += 1;
                    continue;
                }
                return self.evaluate_preserving_intersection_branch_alias(cond.true_type);
            }

            if !extends_has_infer
                && check_type == extends_type
                && self.type_is_compound_generic(cond.extends_type)
            {
                let true_type = self.evaluate(cond.true_type);
                let false_type = self.evaluate(cond.false_type);
                return self.interner().conditional(ConditionalType {
                    check_type,
                    extends_type: cond.extends_type,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                });
            }

            // Step 2b: Non-naked compound type parameter deferral.
            // When the check_type is a compound type containing type parameters
            // (e.g., `T & U`, `keyof T`, `T[K]`), the conditional must be deferred.
            // Unlike a naked TypeParameter (handled in Step 2), compound types like
            // intersections won't be caught by the TypeParameter check above.
            //
            // We check the RAW (pre-evaluation) check_type because evaluation may
            // collapse the structure (e.g., `Intersection(Lazy, Lazy)` → `Lazy`).
            // We exclude naked Lazy (single type params) since those should have been
            // caught by Step 2, or will be handled by the subtype check deferral.
            //
            // Only defer when extends_type has no infer patterns (those need pattern
            // matching first — Step 3 handles them with its own deferral logic).
            if !extends_has_infer
                && (self.type_is_compound_generic(cond.check_type)
                    || (self.type_is_generic_tuple(cond.check_type)
                        && self.type_contains_never(cond.extends_type))
                    || (self.type_is_generic_tuple(cond.check_type)
                        && self.type_has_nested_generic_tuple(cond.extends_type)))
            {
                return self.interner().conditional(*cond);
            }

            // Step 2b': Deferred conditional as check_type.
            //
            // When check_type evaluates to a deferred conditional containing type
            // parameters (e.g., `Extract<T, Foo>` → `T extends Foo ? T : never`),
            // the outer conditional is indeterminate: the inner conditional could
            // evaluate to any type once T is instantiated, so we can't determine
            // whether it satisfies extends_type.
            //
            // Example: `Extract<Extract<T, Foo>, Bar>`
            //   check_type = (T extends Foo ? T : never)  [deferred]
            //   extends_type = Bar
            //   Until T is known, we can't tell if Extract<T, Foo> <: Bar.
            //
            // We evaluate true/false types so the deferred conditional has
            // consistent types (enables Extract pattern recognition in the
            // subtype checker's get_conditional_constraint).
            if !extends_has_infer
                && matches!(
                    self.interner().lookup(check_type),
                    Some(TypeData::Conditional(_))
                )
                && crate::visitor::contains_type_parameters(self.interner(), check_type)
            {
                let true_type = self.evaluate(cond.true_type);
                let false_type = self.evaluate(cond.false_type);
                return self.interner().conditional(ConditionalType {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                });
            }

            if !extends_has_infer
                && extends_has_type_params
                && crate::visitor::contains_free_type_parameters(self.interner(), cond.check_type)
                && self
                    .resolve_generic_constraint(cond.check_type)
                    .is_none_or(|constraint| constraint == cond.check_type)
            {
                let true_type = self.evaluate(cond.true_type);
                let false_type = self.evaluate(cond.false_type);
                return self.interner().conditional(ConditionalType {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                });
            }

            // Step 3: Perform subtype check or infer pattern matching
            // Reuse pre-allocated bindings/visited from outside the loop
            loop_bindings.clear();
            loop_visited.clear();

            if extends_has_infer {
                // PERF: Only allocate SubtypeChecker when infer matching is needed.
                let mut checker = self.conditional_subtype_checker();
                checker.allow_bivariant_rest = true;
                if cond.extends_type != extends_type
                    && self.type_contains_infer(cond.extends_type)
                    && self.match_infer_pattern(
                        check_type,
                        cond.extends_type,
                        &mut loop_bindings,
                        &mut loop_visited,
                        &mut checker,
                    )
                    && !loop_bindings.is_empty()
                {
                    let substituted_true = self.substitute_infer(cond.true_type, &loop_bindings);
                    return self.evaluate(substituted_true);
                }
                loop_bindings.clear();
                loop_visited.clear();
                if cond.extends_type != extends_type
                    && self.type_contains_infer(cond.extends_type)
                    && let Some(alias) = self.interner().get_display_alias(check_type)
                    && alias != check_type
                    && self.match_infer_pattern(
                        alias,
                        cond.extends_type,
                        &mut loop_bindings,
                        &mut loop_visited,
                        &mut checker,
                    )
                    && !loop_bindings.is_empty()
                {
                    let substituted_true = self.substitute_infer(cond.true_type, &loop_bindings);
                    return self.evaluate(substituted_true);
                }
                loop_bindings.clear();
                loop_visited.clear();
                if self.type_contains_infer(cond.extends_type)
                    && let Some(alias) = self.interner().get_display_alias(check_type)
                    && alias != check_type
                    && self.match_infer_pattern(
                        alias,
                        cond.extends_type,
                        &mut loop_bindings,
                        &mut loop_visited,
                        &mut checker,
                    )
                    && !loop_bindings.is_empty()
                {
                    let substituted_true = self.substitute_infer(cond.true_type, &loop_bindings);
                    return self.evaluate(substituted_true);
                }
                loop_bindings.clear();
                loop_visited.clear();
                if self.match_infer_pattern(
                    check_type,
                    extends_type,
                    &mut loop_bindings,
                    &mut loop_visited,
                    &mut checker,
                ) && !loop_bindings.is_empty()
                {
                    let substituted_true = self.substitute_infer(cond.true_type, &loop_bindings);
                    match self.try_dispatch_tail_call(
                        substituted_true,
                        &mut tail_application_branch,
                        tail_recursion_count,
                    ) {
                        TailCallStep::Continue { type_id, cond } => {
                            current_cond_type = type_id;
                            current_cond = cond;
                            tail_recursion_count += 1;
                            continue;
                        }
                        TailCallStep::InstantiatedApp { original, resolved } => {
                            self.apparent_conditional_branch = Some(original);
                            return self.evaluate_preserving_tail_application_branch_alias(
                                resolved,
                                Some(original),
                            );
                        }
                        TailCallStep::BareApplication | TailCallStep::NoTailCall => {}
                    }
                    // Direct Application branch (runs even at limit).
                    if matches!(
                        self.interner().lookup(substituted_true),
                        Some(TypeData::Application(_))
                    ) {
                        self.apparent_conditional_branch = Some(substituted_true);
                        return self.evaluate_preserving_tail_application_branch_alias(
                            substituted_true,
                            Some(substituted_true),
                        );
                    }
                    return self.evaluate(substituted_true);
                }

                let re_evaluated_check = self.evaluate(check_type);
                if re_evaluated_check != check_type {
                    loop_bindings.clear();
                    loop_visited.clear();
                    let mut checker = self.conditional_subtype_checker();
                    checker.allow_bivariant_rest = true;
                    if self.match_infer_pattern(
                        re_evaluated_check,
                        extends_type,
                        &mut loop_bindings,
                        &mut loop_visited,
                        &mut checker,
                    ) && !loop_bindings.is_empty()
                    {
                        let substituted_true =
                            self.substitute_infer(cond.true_type, &loop_bindings);
                        return self.evaluate(substituted_true);
                    }
                }

                if self.infer_pattern_has_unresolved_application(cond.extends_type)
                    && (extends_type == cond.extends_type
                        || self.infer_pattern_has_unresolved_application(extends_type))
                {
                    // Lib-backed patterns can be seen before their base is
                    // resolved. Keep the conditional deferred rather than
                    // caching the false branch for e.g. Array<infer U>.
                    return self.interner().conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type: cond.true_type,
                        false_type: cond.false_type,
                        is_distributive: cond.is_distributive,
                    });
                }

                // Infer pattern didn't match on check_type directly.
                // If check_type is a generic type (IndexAccess, KeyOf, etc.) containing
                // type parameters, try matching with the constraint/upper bound of the
                // check_type. For example, ReturnType<T[M]> where T extends FunctionsObj<T>:
                // T[M]'s constraint resolves to () => unknown, which matches (...args) => infer R.
                //
                // If the constraint ALSO fails to match, take the false branch (the check_type's
                // constraint is the most permissive instantiation, so a match failure is definitive).
                // If the constraint matches, defer — the actual type may match differently once
                // instantiated.
                if crate::visitor::contains_type_parameters(self.interner(), check_type) {
                    let mut checked_concrete_constraint = false;
                    let constraint = self.resolve_generic_constraint(check_type);
                    if let Some(constraint) = constraint
                        && constraint != check_type
                    {
                        checked_concrete_constraint = true;
                        let mut bindings2 = FxHashMap::default();
                        let mut visited2 = InferPatternVisited::default();
                        let mut checker2 = self.conditional_subtype_checker();
                        checker2.allow_bivariant_rest = true;
                        if self.match_infer_pattern(
                            constraint,
                            extends_type,
                            &mut bindings2,
                            &mut visited2,
                            &mut checker2,
                        ) {
                            // Constraint matched the infer pattern. Take the true branch
                            // with the inferred type bindings from the constraint match.
                            // Example: ReturnType<T[M]> where T[M]'s constraint is () => unknown
                            // matches (...args) => infer R, giving R = unknown.
                            // True branch is R, so result is unknown.
                            let substituted_true =
                                self.substitute_infer(cond.true_type, &bindings2);
                            return self.evaluate(substituted_true);
                        }
                    }

                    if !checked_concrete_constraint {
                        // No narrower constraint than the check type itself was
                        // available, so the most permissive form of the check is
                        // the wildcard instantiation (every free parameter ->
                        // `any`). tsc's `getConditionalType` only defers a
                        // failed-match generic check when that permissive form
                        // could still satisfy the extends side; when even the
                        // permissive form fails the relation, the false branch is
                        // definitive (e.g. `OK<T> extends { then(...): any }` —
                        // `OK<any>` has no `then`, so distributing `Awaited<…>`
                        // over a union member like `OK<T>` must reduce to that
                        // member rather than leaving a raw conditional). Fall
                        // through to the false branch in that definitive case;
                        // otherwise keep the conditional deferred.
                        if self.is_depth_detection_pass()
                            || !self.permissive_false_branch_is_definitive(check_type, extends_type)
                        {
                            return self.interner().conditional(ConditionalType {
                                check_type,
                                extends_type,
                                true_type: cond.true_type,
                                false_type: cond.false_type,
                                is_distributive: cond.is_distributive,
                            });
                        }
                    }
                }

                // Infer match failed (and constraint doesn't match either).
                // If check_type is an unresolved TypeQuery, defer rather than eagerly
                // taking the false branch.
                if matches!(
                    self.interner().lookup(check_type),
                    Some(TypeData::TypeQuery(_))
                ) {
                    let true_type = self.evaluate(cond.true_type);
                    let false_type = self.evaluate(cond.false_type);
                    return self.interner().conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    });
                }

                // Infer match failed. If the check type is still generic the
                // failure is only definitive when it also fails under the
                // permissive instantiation (every type parameter replaced by
                // `any` — tsc's `getPermissiveInstantiation` gate in
                // `getConditionalType`); otherwise instantiation could still
                // make the pattern match, so the conditional stays deferred.
                // The TS2589 depth-detection pass is exempt: it evaluates
                // alias bodies with their parameters left free precisely so
                // the recursive branch is driven and the recursion guard can
                // observe the re-applied alias.
                if !self.is_depth_detection_pass()
                    && crate::type_queries::is_generic_conditional_check_type(
                        self.interner(),
                        check_type,
                    )
                    && !self.permissive_false_branch_is_definitive(check_type, extends_type)
                {
                    return self.interner().conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type: cond.true_type,
                        false_type: cond.false_type,
                        is_distributive: cond.is_distributive,
                    });
                }

                // Infer match failed, but the (non-generic) check type is an
                // indexed access whose interface base the resolver cannot resolve
                // yet — an unregistered user interface in this evaluation context
                // (the resolution-order window of #13980). The pattern was matched
                // against an unreduced meta-type, so the `false` is
                // schedule-dependent: the identical conditional matches its true
                // branch once the interface is registered by a consuming
                // call/relation path. Defer instead of collapsing to the false
                // branch (#14164). The depth-detection pass is exempt for the same
                // reason as the generic guard above.
                if !self.is_depth_detection_pass()
                    && self.index_access_blocks_on_unresolved_interface(check_type)
                {
                    return self.deferred_conditional(cond, check_type, extends_type);
                }

                // Infer match failed — take the false branch.
                match self.try_dispatch_tail_call(
                    cond.false_type,
                    &mut tail_application_branch,
                    tail_recursion_count,
                ) {
                    TailCallStep::Continue { type_id, cond } => {
                        current_cond_type = type_id;
                        current_cond = cond;
                        tail_recursion_count += 1;
                        continue;
                    }
                    TailCallStep::InstantiatedApp { original, resolved } => {
                        self.apparent_conditional_branch = Some(original);
                        return self.evaluate_preserving_tail_application_branch_alias(
                            resolved,
                            Some(original),
                        );
                    }
                    TailCallStep::BareApplication => {
                        self.apparent_conditional_branch = Some(cond.false_type);
                    }
                    TailCallStep::NoTailCall => {}
                }
                return self.evaluate(cond.false_type);
            }

            // A genuine error settles the conditional to its false branch; an
            // unresolved cross-arena reference instead defers it (see
            // `resolve_conditional_error_or_unresolved`).
            if let Some(result) =
                self.resolve_conditional_error_or_unresolved(cond, check_type, extends_type)
            {
                return result;
            }

            let relation = self.conditional_subtype_relation(check_type, extends_type);
            trace!(
                check = check_type.0,
                extends = extends_type.0,
                ?relation,
                "conditional subtype check result"
            );

            // The relation's `false` depended on a `Lazy(DefId)` body that was
            // not yet registered (re-entrant lib/interface resolution). tsc
            // treats it as undetermined and defers, rather than committing the
            // spurious false branch (issue #14238).
            if relation == BranchRelation::Undetermined {
                return self.deferred_conditional(cond, check_type, extends_type);
            }
            let is_sub = relation == BranchRelation::Holds;

            // A conditional whose evaluated CHECK type is an opaque, resolver-less
            // `Application` must defer rather than vacuously take its true branch
            // (#13609). See `defer_resolver_less_application_check`.
            if let Some(deferred) =
                self.defer_resolver_less_application_check(cond, check_type, extends_type, is_sub)
            {
                return deferred;
            }

            let result_branch = if is_sub {
                // T <: U -> true branch
                cond.true_type
            } else if (extends_has_type_params
                // A *concrete* check against a generic extends still resolves to
                // the false branch when the relation also fails under the
                // permissive instantiation (every extends type parameter -> `any`,
                // tsc's `getPermissiveInstantiation` gate). e.g.
                // `[] extends [T, ...T[]] ? "yes" : "no"` is `"no"` because `[]`
                // is not a `[any, ...any[]]` regardless of `T`. Keep the exception
                // to tuple-like extends operands: richer generic extends operands,
                // such as React's `ElementType extends T` component-props
                // conditionals, must remain deferred so contextual JSX typing can
                // infer `T` before the conditional chooses a branch.
                && (!self.generic_extends_can_use_permissive_false_branch(extends_type)
                    || extends_has_infer
                    || self.is_depth_detection_pass()
                    || !self.permissive_false_branch_is_definitive(check_type, extends_type)))
                // tsc parity (`getConditionalType`): a conditional whose
                // effective check type is still generic — instantiable flags,
                // or a type reference/tuple/template whose arguments are
                // generic — only resolves to the false branch when the
                // relation also fails under the permissive instantiation
                // (every type parameter replaced by `any`, tsc's
                // `getPermissiveInstantiation` gate); otherwise it stays
                // deferred until instantiation makes the check type concrete.
                // This also covers deferred wrappers over *unresolved*
                // references (`keyof Lazy(D)`), which under parallel fresh
                // checking must defer rather than yield a schedule-dependent
                // definitive false. The TS2589 depth-detection pass is exempt
                // so unconditionally-recursive aliases still drive their
                // recursive branch and surface the depth error.
                || (!self.is_depth_detection_pass()
                    && crate::type_queries::is_generic_conditional_check_type(
                        self.interner(),
                        check_type,
                    )
                    && !self.permissive_false_branch_is_definitive(check_type, extends_type))
                // Also check if the evaluated check_type is a direct Lazy reference
                // (or a union/intersection of Lazy refs). Type parameters in generic
                // function bodies are Lazy(DefId) and contains_type_parameters doesn't
                // see through them. A direct Lazy check_type means the whole type is
                // unresolved (e.g., `T & U` becomes Lazy(DefId)), so the conditional
                // result is indeterminate. Don't defer for wrapped Lazy (like KeyOf(Lazy))
                // where the wrapper type provides enough info for a determinate result.
                || matches!(self.interner().lookup(check_type), Some(TypeData::Lazy(_)))
                || matches!(self.interner().lookup(extends_type), Some(TypeData::Lazy(_)))
                // An Application in the extends position that survived evaluation means
                // the current resolver lacks the generic body (e.g., a lib type like
                // Pick, Readonly, or Required not yet known to the TypeEnvironment
                // resolver). Taking the false branch would be incorrect when the
                // Application could expand to a type that `check_type` does satisfy.
                // Defer so a later resolver pass (CheckerContext) can expand it.
                || matches!(self.interner().lookup(extends_type), Some(TypeData::Application(_)))
                // A `keyof Lazy(DefId)` the resolver cannot expand keeps part of
                // the key space opaque, so a concrete `literal extends keyof Ref`
                // check cannot be definitively false. See
                // `relation_has_unresolvable_keyof_lazy` (#14337).
                || self.relation_has_unresolvable_keyof_lazy(check_type, extends_type)
            {
                // Subtype check failed, but either side contains unresolved type
                // parameters or lazy references. The result is indeterminate: once
                // the type parameters are instantiated, the relationship might change.
                // Examples:
                //   `number extends T ? X : Y` — T could be `number`
                //   `T & U extends string ? X : Y` — T & U could be `string`
                // Defer the conditional instead of eagerly taking the false branch.
                return self.interner().conditional(ConditionalType {
                    check_type,
                    extends_type,
                    true_type: cond.true_type,
                    false_type: cond.false_type,
                    is_distributive: cond.is_distributive,
                });
            } else {
                // Types are definitely not in a subtype relationship and extends_type
                // has no type parameters — take the false branch.
                cond.false_type
            };

            match self.try_dispatch_tail_call(
                result_branch,
                &mut tail_application_branch,
                tail_recursion_count,
            ) {
                TailCallStep::Continue { type_id, cond } => {
                    current_cond_type = type_id;
                    current_cond = cond;
                    tail_recursion_count += 1;
                    continue;
                }
                TailCallStep::InstantiatedApp { original, resolved } => {
                    self.apparent_conditional_branch = Some(original);
                    return self.evaluate_preserving_tail_application_branch_alias(
                        resolved,
                        Some(original),
                    );
                }
                TailCallStep::BareApplication => {
                    self.apparent_conditional_branch = Some(result_branch);
                }
                TailCallStep::NoTailCall => {}
            }
            return self.evaluate_preserving_tail_application_branch_alias(
                result_branch,
                tail_application_branch,
            );
        }
    }

    fn evaluate_preserving_tail_application_branch_alias(
        &mut self,
        branch: TypeId,
        tail_application_branch: Option<TypeId>,
    ) -> TypeId {
        let evaluated = self.evaluate_preserving_intersection_branch_alias(branch);
        if let Some(application_branch) = tail_application_branch
            && evaluated != application_branch
            && self.is_concrete_application_branch(application_branch, evaluated)
        {
            self.interner()
                .store_display_alias_preferring_application(evaluated, application_branch);
        }
        evaluated
    }

    fn evaluate_preserving_intersection_branch_alias(&mut self, branch: TypeId) -> TypeId {
        let evaluated = self.evaluate(branch);
        if evaluated != branch {
            if self.is_concrete_application_branch(branch, evaluated) {
                self.interner()
                    .store_display_alias_preferring_application(evaluated, branch);
            } else if self.is_concrete_application_led_intersection(branch) {
                self.interner().store_display_alias(evaluated, branch);
            }
        }
        evaluated
    }

    fn is_concrete_application_branch(&self, branch: TypeId, evaluated: TypeId) -> bool {
        self.is_concrete_application_display_branch(branch, evaluated)
    }

    pub(in crate::evaluation) fn is_displayable_conditional_branch_result(
        interner: &dyn crate::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        matches!(
            interner.lookup(type_id),
            Some(
                TypeData::Application(_)
                    | TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::Intersection(_)
                    | TypeData::Mapped(_)
            )
        )
    }

    fn is_concrete_application_led_intersection(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(TypeData::Intersection(members)) = self.interner().lookup(type_id) else {
            return false;
        };
        let members = self.interner().type_list(members);
        matches!(
            members
                .first()
                .and_then(|&member| self.interner().lookup(member)),
            Some(TypeData::Application(_))
        ) && !crate::type_queries::contains_generic_type_parameters_db(self.interner(), type_id)
    }

    /// Resolve the base constraint of a generic type by substituting type parameters
    /// with their constraints. This is used to determine if a generic `check_type` COULD
    /// match an extends pattern with infer types.
    ///
    /// For example:
    /// - `T` where `T extends () => unknown` → `() => unknown`
    /// - `T[M]` where `T extends { [K in keyof T]: () => unknown }` → resolves through index access
    /// - `KeyOf(T)` → stays as-is (keyof constraints are complex)
    ///
    /// Returns `Some(resolved)` if a constraint could be computed, `None` otherwise.
    fn resolve_generic_constraint(&mut self, type_id: TypeId) -> Option<TypeId> {
        match self.interner().lookup(type_id) {
            Some(TypeData::TypeParameter(param)) => param.constraint,
            Some(TypeData::IndexAccess(obj, idx)) => {
                // For MappedType[TypeParam], if the TypeParam's constraint matches
                // the mapped type's key constraint, return the template type.
                // Example: { [K in keyof T]: () => unknown }[M] where M extends keyof T
                // → () => unknown
                if let Some(TypeData::Mapped(mapped_id)) = self.interner().lookup(obj) {
                    let mapped = self.interner().get_mapped(mapped_id);
                    if mapped.name_type.is_none() {
                        let evaluated_template = self.evaluate(mapped.template);
                        if !crate::visitor::contains_type_parameters(
                            self.interner(),
                            evaluated_template,
                        ) {
                            return Some(evaluated_template);
                        }
                    }
                }
                // Fallback: try resolving the object type's constraint
                let obj_constraint = self.resolve_generic_constraint(obj);
                if let Some(obj_constraint) = obj_constraint
                    && obj_constraint != obj
                {
                    let resolved = self.evaluate(self.interner().index_access(obj_constraint, idx));
                    if resolved != type_id
                        && !crate::visitor::contains_type_parameters(self.interner(), resolved)
                    {
                        return Some(resolved);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn try_expand_application_for_conditional_check(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.with_optional_meta_rereduce_recursion_identity(type_id, type_id, |evaluator| {
            evaluator.try_expand_application_for_conditional_check_inner(type_id)
        })
    }

    fn try_expand_application_for_conditional_check_inner(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let Some(TypeData::Application(app_id)) = self.interner().lookup(type_id) else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        // For TypeQuery bases (e.g. `typeof ClassName<T>`), always use
        // `resolve_type_query` to get the CONSTRUCTOR type. `resolve_lazy` would
        // return the INSTANCE type for classes (via `class_instance_types`), so
        // `InstanceType<typeof Class<T>>` would fail its constructor constraint check.
        let (def_id_opt, resolved, base_is_type_query) = match self.interner().lookup(app.base)? {
            TypeData::Lazy(def_id) => {
                let r = self.resolver().resolve_lazy(def_id, self.interner())?;
                (Some(def_id), r, false)
            }
            TypeData::UnresolvedTypeName(atom) => {
                let name = self.interner().resolve_atom(atom);
                let def_id = self.resolver().resolve_unresolved_type_name(&name)?;
                let r = self.resolver().resolve_lazy(def_id, self.interner())?;
                (Some(def_id), r, false)
            }
            TypeData::TypeQuery(sym_ref) => {
                let def_id_opt = self.resolver().symbol_to_def_id(sym_ref);
                let r = self
                    .resolver()
                    .resolve_type_query(sym_ref, self.interner())?;
                (def_id_opt, r, true)
            }
            _ => return None,
        };
        if app.args.len() == 1
            && let Some(TypeData::IndexAccess(obj, idx)) = self.interner().lookup(resolved)
            && let Some(TypeData::TypeParameter(tp)) = self.interner().lookup(obj)
        {
            let subst = TypeSubstitution::single(tp.name, app.args[0]);
            let instantiated_obj =
                crate::instantiation::instantiate::instantiate_type(self.interner(), obj, &subst);
            let evaluated_obj = self.evaluate(instantiated_obj);
            let evaluated_idx = self.evaluate(idx);
            let direct = self.evaluate_index_access(evaluated_obj, evaluated_idx);
            if direct != resolved && direct != type_id {
                return Some(direct);
            }
        }
        // For TypeQuery-based Applications (e.g. `typeof ClassExpr<T>`), use
        // per-signature instantiation so that the class type parameters stored in
        // `sig.type_params` are CONSUMED rather than SHADOWED. The standard
        // `instantiate_generic` path calls `TypeInstantiator` which calls
        // `enter_shadowing_scope(&sig.type_params)`, blocking substitution of those
        // names from an outer substitution.
        if base_is_type_query {
            let instantiate_result = self.try_instantiate_callable_type_params(resolved, &app.args);
            if let Some(specialized) = instantiate_result {
                let evaluated = self.evaluate(specialized);
                return (evaluated != type_id).then_some(evaluated);
            }
            // If per-sig instantiation didn't apply (e.g., resolved is not a
            // Callable), fall through to the generic path below.
        }

        let type_params = def_id_opt
            .and_then(|def_id| self.resolver().get_lazy_type_params(def_id))
            .filter(|params| params.len() == app.args.len())
            .unwrap_or_else(|| self.extract_type_params_from_type(resolved).to_vec());
        if type_params.len() != app.args.len() {
            return None;
        }
        let instantiated = instantiate_generic_cached(
            self.interner(),
            self.query_db(),
            resolved,
            &type_params,
            &app.args,
        );
        if let Some(TypeData::IndexAccess(obj, idx)) = self.interner().lookup(instantiated) {
            let evaluated_obj = self.evaluate(obj);
            let evaluated_idx = self.evaluate(idx);
            let direct = self.evaluate_index_access(evaluated_obj, evaluated_idx);
            if direct != instantiated && direct != type_id {
                return Some(direct);
            }
        }
        let evaluated = self.evaluate(instantiated);
        (evaluated != type_id).then_some(evaluated)
    }

    /// Instantiate a `Callable` type's signatures using per-signature substitution.
    ///
    /// When an instantiation expression like `typeof ClassExpr<T>` or
    /// `typeof fn<T>` appears as the check type of a conditional like
    /// `InstanceType<typeof ClassExpr<T>>` or `ReturnType<typeof fn<T>>`, the
    /// generic signature's type parameters live inside `sig.type_params` on
    /// either the constructor signatures (classes) or the call signatures
    /// (regular generic functions/methods). The standard `instantiate_generic`
    /// path calls `TypeInstantiator` which calls
    /// `enter_shadowing_scope(&sig.type_params)`, adding those names to the
    /// shadowed set and preventing them from being substituted.
    ///
    /// This method bypasses that by building a substitution from each
    /// signature's own `type_params` and applying it to the signature's parts
    /// individually (the same approach as the checker's `instantiate_signature`).
    /// Type params are consumed (set to `Vec::new()`) in the output signatures
    /// so the resulting Callable is fully concrete with respect to the supplied
    /// `type_args`.
    ///
    /// Both construct and call signatures are instantiated so that
    /// `typeof fn<Args>` (where `fn`'s `<T>` lives on its call signature) and
    /// `typeof ClassExpr<Args>` (where `<T>` lives on the constructor) follow
    /// the same path. Without this, the application stays opaque and
    /// downstream `ReturnType<...>`, `Parameters<...>`, and `infer` patterns
    /// fail to substitute through to a mapped-type-bearing return type — the
    /// homomorphic mapped result silently degrades to `any`, losing
    /// `readonly`/`?` modifier intent.
    ///
    /// Returns `Some(new_callable_id)` when at least one signature was
    /// successfully instantiated; returns `None` otherwise (no
    /// arity-matching signature, not a Callable, etc.).
    pub(in crate::evaluation) fn try_instantiate_callable_type_params(
        &mut self,
        callable_id: TypeId,
        type_args: &[TypeId],
    ) -> Option<TypeId> {
        let cs_id = match self.interner().lookup(callable_id)? {
            TypeData::Callable(cs_id) => cs_id,
            _ => return None,
        };
        let shape = self.interner().callable_shape(cs_id);

        fn instantiate_sig(
            interner: &dyn crate::construction::TypeDatabase,
            sig: &CallSignature,
            type_args: &[TypeId],
        ) -> Option<CallSignature> {
            if sig.type_params.len() != type_args.len() {
                return None;
            }
            let subst =
                TypeSubstitution::from_signature_args(interner, &sig.type_params, type_args);
            let params: Vec<ParamInfo> = sig
                .params
                .iter()
                .map(|p| ParamInfo {
                    type_id: instantiate_type(interner, p.type_id, &subst),
                    ..*p
                })
                .collect();
            let return_type = instantiate_type(interner, sig.return_type, &subst);
            let this_type = sig.this_type.map(|t| instantiate_type(interner, t, &subst));
            // Type predicates can reference the consumed type params (e.g.
            // `is T` in `<T>(x: any): x is T`); substitute them too so a
            // predicate of the form `is T` becomes `is <concrete arg>`.
            let type_predicate = sig.type_predicate.as_ref().map(|p| {
                let mut predicate = *p;
                predicate.type_id = predicate
                    .type_id
                    .map(|t| instantiate_type(interner, t, &subst));
                predicate
            });
            Some(CallSignature {
                type_params: Vec::new(),
                params,
                return_type,
                this_type,
                type_predicate,
                is_method: sig.is_method,
                declaration_group: sig.declaration_group,
            })
        }

        fn instantiate_sig_list(
            interner: &dyn crate::construction::TypeDatabase,
            sigs: &[CallSignature],
            type_args: &[TypeId],
        ) -> (Vec<CallSignature>, bool) {
            let mut out = Vec::with_capacity(sigs.len());
            let mut changed = false;
            for sig in sigs {
                match instantiate_sig(interner, sig, type_args) {
                    Some(new_sig) => {
                        changed = true;
                        out.push(new_sig);
                    }
                    None => out.push(sig.clone()),
                }
            }
            (out, changed)
        }

        let (new_construct_sigs, construct_changed) =
            instantiate_sig_list(self.interner(), &shape.construct_signatures, type_args);
        let (new_call_sigs, call_changed) =
            instantiate_sig_list(self.interner(), &shape.call_signatures, type_args);
        if !construct_changed && !call_changed {
            return None;
        }

        let new_shape = CallableShape {
            construct_signatures: new_construct_sigs,
            call_signatures: new_call_sigs,
            properties: shape.properties.to_vec(),
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: shape.symbol,
            is_abstract: shape.is_abstract,
        };
        Some(self.interner().callable(new_shape))
    }

    /// Fallback for `evaluate_application` when the base has no `DefId`.
    ///
    /// An instantiation expression `base<Args>` over a generic *value* (a
    /// function or a `const`/`let`/`var` bound to one) reaches evaluation
    /// without a type-space `DefId`, so [`Self::resolve_application_def_id`]
    /// returns `None` and the application lands here. Two base shapes occur:
    ///
    /// * `Callable` — `typeof f<Args>` already lowered to the function's
    ///   callable shape (value-position annotations resolve the query eagerly).
    /// * `TypeQuery(sym)` — the lazy instantiation-expression form preserved by
    ///   type-argument positions (`ReturnType<typeof f<Args>>`, `Parameters<…>`,
    ///   `infer` patterns). The query base is kept intact there, so it must be
    ///   resolved to the underlying callable before instantiation.
    ///
    /// Both shapes instantiate every type-parameter-bearing signature (call and
    /// construct) via [`Self::try_instantiate_callable_type_params`] so
    /// downstream `ReturnType`/`Parameters`/`infer` patterns see the substituted
    /// function shape rather than an opaque application that silently degrades to
    /// `any`.
    ///
    /// When no signature consumes the type arguments the two base shapes differ:
    ///
    /// * A `Callable` base only reaches here after the checker's
    ///   instantiation-expression applicability gate
    ///   (`apply_instantiation_expression_type_arguments`) accepted it and
    ///   eagerly specialized the value to a concrete callable that was then
    ///   re-wrapped as `Application(callable, [X])`. The leftover arguments are
    ///   vestigial, so the application unwraps to the callable itself — exactly
    ///   the instantiation expression's type.
    /// * A `TypeQuery(sym)` base has **not** been re-validated for
    ///   arity/applicability here. If no signature consumes its arguments the
    ///   instantiation is invalid (non-generic value, or wrong arity), so the
    ///   application is left opaque rather than unwrapped — matching tsc, which
    ///   errors (TS2635/TS2344) and does not let `ReturnType` / `Parameters`
    ///   observe the value's real return.
    ///
    /// Any other base — or a query that does not resolve to a callable — stays
    /// opaque for a later pass.
    pub(in crate::evaluation) fn evaluate_application_no_def_id(
        &mut self,
        app_id: crate::types::TypeApplicationId,
        original_type_id: TypeId,
    ) -> TypeId {
        let app = self.interner().type_application(app_id);
        if app.args.is_empty() {
            return original_type_id;
        }
        let base_is_callable = matches!(
            self.interner().lookup(app.base),
            Some(TypeData::Callable(_))
        );
        let Some(callable) = self.callable_for_instantiation_base(app.base) else {
            return original_type_id;
        };
        let args = app.args.clone();
        match self.try_instantiate_callable_type_params(callable, &args) {
            // A generic signature consumed the type arguments.
            Some(specialized) => self.evaluate(specialized),
            // No signature consumed them: vestigial args on an already-validated
            // `Callable` base unwrap to the concrete callable; an unvalidated
            // `TypeQuery` base stays opaque so invalid inline instantiations keep
            // their tsc TS2635/TS2344 parity.
            None if base_is_callable => self.evaluate(callable),
            None => original_type_id,
        }
    }

    /// Resolve the callable type backing an instantiation-expression base
    /// (`base<Args>`) that reached evaluation without a `DefId`.
    ///
    /// Returns a `Callable` base unchanged, or resolves a `TypeQuery(sym)` on a
    /// generic function/`const` value to its callable shape (preferring the
    /// value/constructor type from `resolve_type_query`, falling back to
    /// `resolve_ref`). Any other base — or a query that does not resolve to a
    /// callable — yields `None`, keeping the application opaque.
    fn callable_for_instantiation_base(&self, base: TypeId) -> Option<TypeId> {
        match self.interner().lookup(base)? {
            TypeData::Callable(_) => Some(base),
            TypeData::TypeQuery(sym_ref) => {
                let resolved = self
                    .resolver()
                    .resolve_type_query(sym_ref, self.interner())
                    .or_else(|| self.resolver().resolve_ref(sym_ref, self.interner()))?;
                matches!(
                    self.interner().lookup(resolved),
                    Some(TypeData::Callable(_))
                )
                .then_some(resolved)
            }
            _ => None,
        }
    }

    /// Specialize a `typeof f<Args>` / `typeof Class<Args>` instantiation
    /// expression whose base is a `TypeQuery` that resolved (via a `DefId`) to
    /// a `Callable`.
    ///
    /// The resolved callable's call/construct signatures DECLARE the applied
    /// type params, so they must be CONSUMED by per-signature instantiation
    /// ([`Self::try_instantiate_callable_type_params`]). Routing such a base
    /// through the alias-style known-params path in `evaluate_application_body`
    /// instead would `instantiate_generic` the callable body and
    /// `enter_shadowing_scope(&sig.type_params)`, treating those names as bound
    /// and cancelling the substitution — freezing the instantiation expression
    /// (e.g. `ReturnType<typeof f<U>>` in a generic method never specializes
    /// and degrades to `unknown`, see #10933). Hoisting this ahead of that
    /// branch matters because a generic *function* value reports `Some([T])`
    /// from the resolver, so it would otherwise take the shadowing path.
    ///
    /// Returns `Some(evaluated)` only when a signature actually consumes the
    /// arguments. `None` (non-`TypeQuery` base, empty args, non-`Callable`
    /// body, or arity mismatch) leaves the caller's existing param-based /
    /// opaque handling unchanged, preserving the opaque-on-invalid
    /// TS2635/TS2344 parity.
    pub(in crate::evaluation) fn try_specialize_typeof_instantiation_expression(
        &mut self,
        ctx: &crate::evaluation::evaluate::application_types::ApplicationEvalContext,
        args: &[TypeId],
    ) -> Option<TypeId> {
        if !ctx.base_is_type_query || args.is_empty() {
            return None;
        }
        let resolved = ctx.resolved?;
        if !matches!(
            self.interner().lookup(resolved),
            Some(TypeData::Callable(_))
        ) {
            return None;
        }
        let specialized = self.try_instantiate_callable_type_params(resolved, args)?;
        Some(self.evaluate(specialized))
    }

    /// Distribute a conditional type over a union.
    /// (A | B) extends U ? X : Y -> (A extends U ? X : Y) | (B extends U ? X : Y)
    pub(crate) fn distribute_conditional(
        &mut self,
        members: &[TypeId],
        original_check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) -> TypeId {
        // Limit distribution to prevent OOM with pathologically large unions.
        if members.len() > MAX_CONDITIONAL_DISTRIBUTION_SIZE {
            self.mark_depth_exceeded_for_request();
            return TypeId::ERROR;
        }

        let mut results: SmallVec<[TypeId; 8]> = SmallVec::with_capacity(members.len());
        // PERF: Track whether all results are identical. If every branch
        // produces the same TypeId (common for `T extends X ? never : T`
        // patterns where all members pass/fail uniformly), we can skip the
        // union construction entirely.
        let mut all_same = true;
        let mut first_result = TypeId::NONE;

        // PERF: Pre-allocate the substitution memo outside the loop.
        // Reusing the same HashMap (with clear() between uses) avoids
        // O(members.len()) allocations for large union distributions.
        let mut memo = FxHashMap::default();

        // PERF: A branch that never references the distribution variable
        // substitutes to itself for every member, so gate the per-member
        // rewrite on one containment walk per branch instead of N full
        // substitution walks. `contains_type_by_id` traverses a superset of
        // the substitution walk's children (it also descends `Mapped` and
        // type-parameter internals), so a `false` answer here is always safe
        // to skip on.
        let branch_references_check = |branch: TypeId| branch == original_check_type;
        let extends_needs_subst = branch_references_check(extends_type)
            || self.cached_contains_type_by_id(extends_type, original_check_type);
        let true_needs_subst = branch_references_check(true_type)
            || self.cached_contains_type_by_id(true_type, original_check_type);
        let false_needs_subst = branch_references_check(false_type)
            || self.cached_contains_type_by_id(false_type, original_check_type);

        for &member in members {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Substitute the specific member if true_type or false_type references the original check_type
            // This handles cases like: NonNullable<T> = T extends null ? never : T
            // When T = A | B, we need (A extends null ? never : A) | (B extends null ? never : B)
            let substituted_extends_type = if extends_needs_subst {
                memo.clear();
                self.substitute_exact_type(extends_type, original_check_type, member, &mut memo)
            } else {
                extends_type
            };
            let substituted_true_type = if true_needs_subst {
                memo.clear();
                self.substitute_exact_type(true_type, original_check_type, member, &mut memo)
            } else {
                true_type
            };
            let substituted_false_type = if false_needs_subst {
                memo.clear();
                self.substitute_exact_type(false_type, original_check_type, member, &mut memo)
            } else {
                false_type
            };

            // Create conditional for this union member
            let member_cond = ConditionalType {
                check_type: member,
                extends_type: substituted_extends_type,
                true_type: substituted_true_type,
                false_type: substituted_false_type,
                is_distributive: false,
            };

            // Recursively evaluate while preserving depth and recursion-identity limits.
            let cond_type = self.interner().conditional(member_cond);
            let result = self.recurse_conditional(cond_type);
            // Check if evaluation hit depth limit
            if result == TypeId::ERROR && self.is_depth_exceeded() {
                return TypeId::ERROR;
            }
            if all_same {
                if first_result == TypeId::NONE {
                    first_result = result;
                } else if result != first_result {
                    all_same = false;
                }
            }
            results.push(result);
        }

        // PERF: If all branches produced the same type, return it directly
        // without constructing a union.
        if all_same && first_result != TypeId::NONE {
            return first_result;
        }

        // Combine results into a union
        self.interner().union_from_slice(&results)
    }
}

#[cfg(test)]
mod tests;
