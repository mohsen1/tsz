//! Conditional type evaluation.
//!
//! Handles TypeScript's conditional types: `T extends U ? X : Y`
//! Including distributive conditional types over union types.

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_generic, instantiate_type_with_infer,
};
use crate::operations::property::PropertyAccessResult;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    ConditionalType, ObjectShapeId, PropertyInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tracing::trace;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Maximum depth for tail-recursive conditional evaluation.
    /// This allows patterns like `type Loop<T> = T extends [...infer R] ? Loop<R> : never`
    /// to work with up to 1000 recursive calls instead of being limited to `MAX_EVALUATE_DEPTH`.
    const MAX_TAIL_RECURSION_DEPTH: usize = 1000;

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
        let mut current_cond = *initial_cond;
        let mut tail_recursion_count = 0;
        // PERF: Pre-allocate bindings and visited sets outside the tail-recursion
        // loop so their capacity is preserved across iterations.
        let mut loop_bindings: FxHashMap<Atom, TypeId> = FxHashMap::default();
        let mut loop_visited: FxHashSet<(TypeId, TypeId)> = FxHashSet::default();
        // Cycle detection for the tail-recursion loop.
        // Tracks (check_type, extends_type) pairs seen during tail calls.
        // When the same pair is encountered again, the conditional is cyclically
        // self-referential (e.g., the true/false branch evaluates back to the
        // same conditional). Without this, libraries like ts-toolbelt that have
        // deeply nested conditional types can cause infinite loops.
        let mut tail_seen: FxHashSet<(TypeId, TypeId, TypeId, TypeId)> = FxHashSet::default();

        loop {
            // Clear any apparent branch signal from the previous iteration so stale
            // signals don't leak into the outer evaluate_application.
            self.apparent_conditional_branch = None;

            // When tail recursion reaches the limit, the type didn't converge.
            // Flag TS2589 and return ERROR to prevent stack overflow.
            // This matches tsc's tail recursion limit of 1000 (instantiationCount).
            if tail_recursion_count >= Self::MAX_TAIL_RECURSION_DEPTH {
                self.mark_depth_exceeded();
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
                self.mark_depth_exceeded();
                return TypeId::ERROR;
            }

            // Pre-evaluation Application-level infer matching.
            // When both check and extends are Applications (e.g., Promise<string> vs
            // Promise<infer U>), match type arguments directly before expanding.
            // After evaluation, Application types become structural Object/Callable types,
            // which may fail structural infer matching for complex interfaces like Promise.
            if let Some(result) = self.try_application_infer_match(cond) {
                return result;
            }

            let mut check_type = self.evaluate(cond.check_type);
            let extends_type = self.evaluate(cond.extends_type);
            if matches!(
                self.interner().lookup(check_type),
                Some(TypeData::Application(_))
            ) && let Some(expanded_check) =
                self.try_expand_application_for_conditional_check(check_type)
            {
                check_type = expanded_check;
            }

            // When check_type is an unresolvable Application (e.g., Promise<string>
            // where Promise is referenced via TypeQuery with no DefId yet), try to
            // resolve it structurally. This is critical for Awaited<T>-style patterns
            // where the conditional needs to see Promise's structural members (like
            // `then`) for infer pattern matching.
            //
            // Uses get_type_params + resolve_ref on the SymbolRef directly, bypassing
            // the DefId path which may not be available yet during lazy evaluation.
            if let Some(TypeData::Application(app_id)) = self.interner().lookup(check_type) {
                let app = self.interner().type_application(app_id);
                if let Some(TypeData::TypeQuery(sym_ref)) = self.interner().lookup(app.base)
                    && let Some(type_params) = self.resolver().get_type_params(sym_ref)
                    && let Some(resolved_base) =
                        self.resolver().resolve_ref(sym_ref, self.interner())
                    && !type_params.is_empty()
                    && type_params.len() == app.args.len()
                {
                    let args = app.args.clone();
                    let expanded_args = self.expand_type_args(&args);
                    let instantiated = crate::instantiation::instantiate::instantiate_generic(
                        self.interner(),
                        resolved_base,
                        &type_params,
                        &expanded_args,
                    );
                    let resolved = self.evaluate(instantiated);
                    if resolved != check_type {
                        check_type = resolved;
                    }
                }
            }

            trace!(
                check_raw = cond.check_type.0,
                check_eval = check_type.0,
                check_key = ?self.interner().lookup(check_type),
                extends_raw = cond.extends_type.0,
                extends_eval = extends_type.0,
                extends_key = ?self.interner().lookup(extends_type),
                "evaluate_conditional"
            );

            // PERF: Cache predicate results for extends_type once per iteration.
            // type_contains_infer is called up to 5 times and contains_type_parameters
            // at least once, each creating fresh FxHashSet/FxHashMap allocations.
            let extends_has_infer = self.type_contains_infer(extends_type);
            let extends_has_type_params =
                crate::visitor::contains_type_parameters(self.interner(), extends_type);

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
                    let mut visited = FxHashSet::default();
                    let mut checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
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
                    check_type, // Pass original check_type for substitution
                    extends_type,
                    cond.true_type,
                    cond.false_type,
                );
            }

            if let Some(TypeData::Infer(info)) = self.interner().lookup(extends_type) {
                if matches!(
                    self.interner().lookup(check_type),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                ) {
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
                    let mut checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
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

            let extends_unwrapped = match self.interner().lookup(extends_type) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => extends_type,
            };
            let check_unwrapped = match self.interner().lookup(check_type) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => check_type,
            };

            // PERF: Single lookup for array/tuple extends patterns with infer
            match self.interner().lookup(extends_unwrapped) {
                Some(TypeData::Array(ext_elem)) => {
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(ext_elem) {
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
                }
                _ => {}
            }

            // Step 2: Check for naked type parameter
            if let Some(TypeData::TypeParameter(param)) = self.interner().lookup(check_type) {
                // Simplification: T extends never ? X : Y → Y
                // A type parameter T cannot extend `never` (only `never` extends `never`),
                // so the conditional always takes the false branch.
                if extends_type == TypeId::NEVER {
                    return self.evaluate(cond.false_type);
                }

                // Simplification: T extends T ? X : Y → X
                // A type parameter always extends itself, so the conditional always takes
                // the true branch.
                if check_type == extends_type {
                    return self.evaluate_preserving_intersection_branch_alias(cond.true_type);
                }

                // If extends_type contains infer patterns and the type parameter has a constraint,
                // try to infer from the constraint. This handles cases like:
                // R extends Reducer<infer S, any> ? S : never
                // where R is constrained to Reducer<any, any>
                if extends_has_infer && let Some(constraint) = param.constraint {
                    let mut checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
                    checker.allow_bivariant_rest = true;
                    let mut bindings = FxHashMap::default();
                    let mut visited = FxHashSet::default();
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
            if check_type == extends_type && !self.type_contains_infer(cond.extends_type) {
                return self.evaluate_preserving_intersection_branch_alias(cond.true_type);
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
            if !extends_has_infer && self.type_is_compound_generic(cond.check_type) {
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

            // Step 3: Perform subtype check or infer pattern matching
            // Reuse pre-allocated bindings/visited from outside the loop
            loop_bindings.clear();
            loop_visited.clear();

            if extends_has_infer {
                // PERF: Only allocate SubtypeChecker when infer matching is needed.
                let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
                checker.allow_bivariant_rest = true;
                if self.match_infer_pattern(
                    check_type,
                    extends_type,
                    &mut loop_bindings,
                    &mut loop_visited,
                    &mut checker,
                ) {
                    let substituted_true = self.substitute_infer(cond.true_type, &loop_bindings);
                    // Check for tail-recursive true branch (e.g., Trim<T> recurses on match):
                    // type Trim<S> = S extends ` ${infer T}` ? Trim<T> : S;
                    // The substituted true branch Trim<T> is an Application that expands
                    // to another Conditional — handle it as a tail call WITHOUT
                    // incrementing the depth guard, controlled by MAX_TAIL_RECURSION_DEPTH.
                    if tail_recursion_count < Self::MAX_TAIL_RECURSION_DEPTH {
                        if let Some(TypeData::Conditional(next_cond_id)) =
                            self.interner().lookup(substituted_true)
                        {
                            let next_cond = self.interner().get_conditional(next_cond_id);
                            current_cond = next_cond;
                            tail_recursion_count += 1;
                            continue;
                        }
                        if let Some(instantiated) =
                            self.try_instantiate_application_for_tail_call(substituted_true)
                        {
                            if let Some(TypeData::Conditional(next_cond_id)) =
                                self.interner().lookup(instantiated)
                            {
                                let next_cond = self.interner().get_conditional(next_cond_id);
                                current_cond = next_cond;
                                tail_recursion_count += 1;
                                continue;
                            }
                            // Not a conditional — evaluate normally.
                            // Signal the intermediate Application for forward display alias.
                            self.apparent_conditional_branch = Some(substituted_true);
                            return self.evaluate(instantiated);
                        }
                    }
                    // Direct Application branch.
                    if matches!(
                        self.interner().lookup(substituted_true),
                        Some(TypeData::Application(_))
                    ) {
                        self.apparent_conditional_branch = Some(substituted_true);
                    }
                    return self.evaluate(substituted_true);
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
                    let constraint = self.resolve_generic_constraint(check_type);
                    if let Some(constraint) = constraint
                        && constraint != check_type
                    {
                        let mut bindings2 = FxHashMap::default();
                        let mut visited2 = FxHashSet::default();
                        let mut checker2 =
                            SubtypeChecker::with_resolver(self.interner(), self.resolver());
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

                // Infer match failed — take the false branch.
                // Check if the false branch is a tail-recursive conditional.
                // IMPORTANT: Check BEFORE calling evaluate to avoid incrementing depth
                if tail_recursion_count < Self::MAX_TAIL_RECURSION_DEPTH {
                    if let Some(TypeData::Conditional(next_cond_id)) =
                        self.interner().lookup(cond.false_type)
                    {
                        let next_cond = self.interner().get_conditional(next_cond_id);
                        current_cond = next_cond;
                        tail_recursion_count += 1;
                        continue;
                    }
                    // Also detect Application that expands to Conditional (common pattern):
                    // type TrimLeft<T> = T extends ` ${infer R}` ? TrimLeft<R> : T;
                    // The false branch may be `TrimLeft<R>` (Application, not Conditional).
                    if let Some(instantiated) =
                        self.try_instantiate_application_for_tail_call(cond.false_type)
                    {
                        if let Some(TypeData::Conditional(next_cond_id)) =
                            self.interner().lookup(instantiated)
                        {
                            let next_cond = self.interner().get_conditional(next_cond_id);
                            current_cond = next_cond;
                            tail_recursion_count += 1;
                            continue;
                        }
                        self.apparent_conditional_branch = Some(cond.false_type);
                        return self.evaluate(instantiated);
                    }
                    if matches!(
                        self.interner().lookup(cond.false_type),
                        Some(TypeData::Application(_))
                    ) {
                        self.apparent_conditional_branch = Some(cond.false_type);
                    }
                }

                // Not a tail-recursive case - evaluate normally
                return self.evaluate(cond.false_type);
            }

            // Subtype check path — use strict checking (no bivariant rest)
            // to match tsc's `isTypeAssignableTo` which respects strictFunctionTypes.
            //
            // PERF: Check the evaluator's conditional_subtype_cache first. Deeply
            // recursive conditional types (DeepReadonly, Compute, etc.) re-check
            // the same (check, extends) pair many times across distributed branches
            // and tail-recursion iterations. Caching avoids redundant structural
            // comparison which dominates the time for these benchmarks.
            let is_sub = if let Some(cached) =
                self.cached_conditional_subtype(check_type, extends_type)
            {
                cached
            } else {
                // Thread-local depth guard: evaluating conditional types can trigger
                // subtype checks that evaluate MORE conditional types, creating an
                // Evaluator → SubtypeChecker → Evaluator → ... chain where each
                // instance has fresh cycle-detection state. Without this global
                // depth limit, recursive generic types like `Vector<T> implements
                // Seq<T>` with `Exclude<T, U>` in overloads cause stack overflow.
                thread_local! {
                    static CONDITIONAL_SUBTYPE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
                }
                let prev_depth = CONDITIONAL_SUBTYPE_DEPTH.with(|d| {
                    let c = d.get();
                    d.set(c + 1);
                    c
                });
                let result = if prev_depth >= 50 {
                    // At excessive depth, conservatively assume not a subtype
                    // (takes the false/else branch of the conditional).
                    // This matches tsc's behavior of returning the deferred
                    // conditional when instantiation depth is exceeded.
                    CONDITIONAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                    false
                } else if Self::is_primitive_vs_function(self.interner(), check_type, extends_type)
                {
                    // Fast-path: primitive types (string, number, boolean, bigint,
                    // symbol) are never subtypes of Function. The structural subtype
                    // checker may incorrectly autobox the primitive to its wrapper
                    // type (String, Number, etc.) and find structural compatibility
                    // with the evaluated Function interface. This fast-path prevents
                    // `string extends Function` from incorrectly taking the true
                    // branch, matching tsc's behavior where primitives never extend
                    // Function.
                    CONDITIONAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                    false
                } else {
                    let mut strict_checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
                    let r = strict_checker.is_subtype_of(check_type, extends_type);
                    CONDITIONAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                    r
                };
                self.cache_conditional_subtype(check_type, extends_type, result);
                result
            };
            trace!(
                check = check_type.0,
                extends = extends_type.0,
                is_subtype = is_sub,
                "conditional subtype check result"
            );
            let result_branch = if is_sub {
                // T <: U -> true branch
                cond.true_type
            } else if extends_has_type_params
                // Also check if the evaluated check_type is a direct Lazy reference
                // (or a union/intersection of Lazy refs). Type parameters in generic
                // function bodies are Lazy(DefId) and contains_type_parameters doesn't
                // see through them. A direct Lazy check_type means the whole type is
                // unresolved (e.g., `T & U` becomes Lazy(DefId)), so the conditional
                // result is indeterminate. Don't defer for wrapped Lazy (like KeyOf(Lazy))
                // where the wrapper type provides enough info for a determinate result.
                || matches!(self.interner().lookup(check_type), Some(TypeData::Lazy(_)))
                || matches!(self.interner().lookup(extends_type), Some(TypeData::Lazy(_)))
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

            // Check if the result branch is directly a conditional for tail-recursion
            // IMPORTANT: Check BEFORE calling evaluate to avoid incrementing depth
            if tail_recursion_count < Self::MAX_TAIL_RECURSION_DEPTH {
                if let Some(TypeData::Conditional(next_cond_id)) =
                    self.interner().lookup(result_branch)
                {
                    let next_cond = self.interner().get_conditional(next_cond_id);
                    current_cond = next_cond;
                    tail_recursion_count += 1;
                    continue;
                }
                // Also detect Application that expands to Conditional (tail-call through
                // type alias like `TrimLeft<R>` which is Application, not Conditional)
                if let Some(instantiated) =
                    self.try_instantiate_application_for_tail_call(result_branch)
                {
                    if let Some(TypeData::Conditional(next_cond_id)) =
                        self.interner().lookup(instantiated)
                    {
                        let next_cond = self.interner().get_conditional(next_cond_id);
                        current_cond = next_cond;
                        tail_recursion_count += 1;
                        continue;
                    }
                    self.apparent_conditional_branch = Some(result_branch);
                    return self.evaluate(instantiated);
                }
                if matches!(
                    self.interner().lookup(result_branch),
                    Some(TypeData::Application(_))
                ) {
                    self.apparent_conditional_branch = Some(result_branch);
                }
            }

            // Not a tail-recursive case - evaluate normally
            return self.evaluate_preserving_intersection_branch_alias(result_branch);
        }
    }

    fn evaluate_preserving_intersection_branch_alias(&mut self, branch: TypeId) -> TypeId {
        let evaluated = self.evaluate(branch);
        if evaluated != branch && self.is_concrete_application_led_intersection(branch) {
            self.interner().store_display_alias(evaluated, branch);
        }
        evaluated
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
        let Some(TypeData::Application(app_id)) = self.interner().lookup(type_id) else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        let def_id = match self.interner().lookup(app.base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref),
            _ => None,
        }?;
        let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;
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
        let type_params = self
            .resolver()
            .get_lazy_type_params(def_id)
            .filter(|params| params.len() == app.args.len())
            .unwrap_or_else(|| self.extract_type_params_from_type(resolved));
        if type_params.len() != app.args.len() {
            return None;
        }
        let instantiated = instantiate_generic(self.interner(), resolved, &type_params, &app.args);
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

    /// Check if this is a primitive type vs Function/callable target.
    ///
    /// Primitive types (string, number, boolean, bigint, symbol) are never
    /// subtypes of `Function` in TypeScript. However, the structural subtype
    /// checker may incorrectly find compatibility when it autoboxes the
    /// primitive to its wrapper type (e.g., `String` has `toString()` and
    /// `length` which partially overlap with `Function`'s structural shape).
    ///
    /// This fast-path prevents false positives like `string extends Function`
    /// evaluating to true in conditional types.
    fn is_primitive_vs_function(
        interner: &dyn crate::TypeDatabase,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> bool {
        use crate::types::IntrinsicKind;
        // Check if source is a primitive type
        let is_primitive = matches!(
            check_type,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        );
        if !is_primitive {
            return false;
        }
        // Check if target is the Function intrinsic or resolved Function interface
        if extends_type == TypeId::FUNCTION {
            return true;
        }
        if let Some(crate::types::TypeData::Intrinsic(IntrinsicKind::Function)) =
            interner.lookup(extends_type)
        {
            return true;
        }
        // Check if the evaluated target is structurally the Function interface
        // (has apply, call, bind properties) — this catches cases where the
        // Function type was resolved from a Lazy(DefId) to its ObjectShape form.
        if let Some(
            crate::types::TypeData::Object(shape_id)
            | crate::types::TypeData::ObjectWithIndex(shape_id),
        ) = interner.lookup(extends_type)
        {
            let shape = interner.object_shape(shape_id);
            if shape.properties.len() <= 20 {
                let apply = interner.intern_string("apply");
                let call = interner.intern_string("call");
                let bind = interner.intern_string("bind");
                let has_apply = shape.properties.iter().any(|p| p.name == apply);
                let has_call = shape.properties.iter().any(|p| p.name == call);
                let has_bind = shape.properties.iter().any(|p| p.name == bind);
                if has_apply && has_call && has_bind {
                    return true;
                }
            }
        }
        false
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
        // Limit distribution to prevent OOM with large unions
        const MAX_DISTRIBUTION_SIZE: usize = 100;
        if members.len() > MAX_DISTRIBUTION_SIZE {
            self.mark_depth_exceeded();
            return TypeId::ERROR;
        }

        let mut results: SmallVec<[TypeId; 8]> = SmallVec::with_capacity(members.len());
        // PERF: Track whether all results are identical. If every branch
        // produces the same TypeId (common for `T extends X ? never : T`
        // patterns where all members pass/fail uniformly), we can skip the
        // union construction entirely.
        let mut all_same = true;
        let mut first_result = TypeId::NONE;

        for &member in members {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Substitute the specific member if true_type or false_type references the original check_type
            // This handles cases like: NonNullable<T> = T extends null ? never : T
            // When T = A | B, we need (A extends null ? never : A) | (B extends null ? never : B)
            let substituted_true_type = if true_type == original_check_type {
                member
            } else {
                true_type
            };
            let substituted_false_type = if false_type == original_check_type {
                member
            } else {
                false_type
            };

            // Create conditional for this union member
            let member_cond = ConditionalType {
                check_type: member,
                extends_type,
                true_type: substituted_true_type,
                false_type: substituted_false_type,
                is_distributive: false,
            };

            // Recursively evaluate via evaluate() to respect depth limits
            let cond_type = self.interner().conditional(member_cond);
            let result = self.evaluate(cond_type);
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

    /// Handle array extends pattern: T extends (infer U)[] ? ...
    fn eval_conditional_array_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        info: TypeParamInfo,
    ) -> TypeId {
        // PERF: Single lookup for type parameter check + inferred extraction
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred = match check_key {
            Some(TypeData::Array(elem)) => Some(elem),
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                let mut parts: SmallVec<[TypeId; 8]> = SmallVec::new();
                for element in elements.iter() {
                    if element.rest {
                        let rest_type = self.rest_element_type(element.type_id);
                        parts.push(rest_type);
                    } else {
                        let elem_type = if element.optional {
                            self.interner().union2(element.type_id, TypeId::UNDEFINED)
                        } else {
                            element.type_id
                        };
                        parts.push(elem_type);
                    }
                }
                if parts.is_empty() {
                    None
                } else {
                    Some(self.interner().union_from_slice(&parts))
                }
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut parts: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    match self.interner().lookup(member) {
                        Some(TypeData::Array(elem)) => parts.push(elem),
                        Some(TypeData::ReadonlyType(inner)) => {
                            let Some(TypeData::Array(elem)) = self.interner().lookup(inner) else {
                                return self.evaluate(cond.false_type);
                            };
                            parts.push(elem);
                        }
                        _ => return self.evaluate(cond.false_type),
                    }
                }
                if parts.is_empty() {
                    None
                } else if parts.len() == 1 {
                    Some(parts[0])
                } else {
                    Some(self.interner().union_from_slice(&parts))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
            if is_union && !cond.is_distributive {
                // For unions in non-distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For single values or distributive conditionals, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        if matches!(
            self.interner().lookup(true_inst),
            Some(TypeData::Application(_))
        ) && crate::type_queries::contains_generic_type_parameters_db(self.interner(), true_inst)
        {
            return true_inst;
        }
        self.evaluate(true_inst)
    }

    /// Handle tuple extends pattern: T extends [infer U] ? ...
    fn eval_conditional_tuple_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_elem: &TupleElement,
        info: TypeParamInfo,
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred = match check_key {
            Some(TypeData::Tuple(check_elements)) => {
                let check_elements = self.interner().tuple_list(check_elements);
                if check_elements.is_empty() {
                    extends_elem.optional.then_some(TypeId::UNDEFINED)
                } else if check_elements.len() == 1 && !check_elements[0].rest {
                    let elem = &check_elements[0];
                    Some(if elem.optional {
                        self.interner().union2(elem.type_id, TypeId::UNDEFINED)
                    } else {
                        elem.type_id
                    })
                } else {
                    None
                }
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_type = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    match self.interner().lookup(member_type) {
                        Some(TypeData::Tuple(check_elements)) => {
                            let check_elements = self.interner().tuple_list(check_elements);
                            if check_elements.is_empty() {
                                if extends_elem.optional {
                                    inferred_members.push(TypeId::UNDEFINED);
                                    continue;
                                }
                                return self.evaluate(cond.false_type);
                            }
                            if check_elements.len() == 1 && !check_elements[0].rest {
                                let elem = &check_elements[0];
                                let elem_type = if elem.optional {
                                    self.interner().union2(elem.type_id, TypeId::UNDEFINED)
                                } else {
                                    elem.type_id
                                };
                                inferred_members.push(elem_type);
                            } else {
                                return self.evaluate(cond.false_type);
                            }
                        }
                        _ => return self.evaluate(cond.false_type),
                    }
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union_from_slice(&inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let Some(filtered) =
                self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
            else {
                let false_inst =
                    instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                return self.evaluate(false_inst);
            };
            inferred = filtered;
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Handle object extends pattern: T extends { prop: infer U } ? ...
    fn eval_conditional_object_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_shape_id: ObjectShapeId,
    ) -> Option<TypeId> {
        let extends_shape = self.interner().object_shape(extends_shape_id);
        let mut infer_props: SmallVec<[(Atom, TypeParamInfo, bool); 4]> = SmallVec::new();
        let mut infer_nested = None;

        for prop in &extends_shape.properties {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(prop.type_id) {
                if infer_nested.is_some() {
                    return None;
                }
                infer_props.push((prop.name, info, prop.optional));
                continue;
            }

            let nested_type = match self.interner().lookup(prop.type_id) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => prop.type_id,
            };
            if let Some(nested_shape_id) = match self.interner().lookup(nested_type) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    Some(shape_id)
                }
                _ => None,
            } {
                let nested_shape = self.interner().object_shape(nested_shape_id);
                let mut nested_infer = None;
                for nested_prop in &nested_shape.properties {
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(nested_prop.type_id)
                    {
                        if nested_infer.is_some() {
                            nested_infer = None;
                            break;
                        }
                        nested_infer = Some((nested_prop.name, info));
                    }
                }
                if let Some((nested_name, info)) = nested_infer {
                    if !infer_props.is_empty() || infer_nested.is_some() {
                        return None;
                    }
                    infer_nested = Some((prop.name, nested_name, info));
                }
            }
        }

        if infer_props.len() == 1 {
            let (prop_name, info, prop_optional) = infer_props[0];
            return Some(self.eval_conditional_object_prop_infer(
                cond,
                check_unwrapped,
                prop_name,
                info,
                prop_optional,
            ));
        }

        if infer_props.len() > 1 {
            return Some(self.eval_conditional_object_multi_prop_infer(
                cond,
                check_unwrapped,
                &infer_props,
            ));
        }

        if let Some((outer_name, inner_name, info)) = infer_nested {
            return Some(self.eval_conditional_object_nested_infer(
                cond,
                check_unwrapped,
                outer_name,
                inner_name,
                info,
            ));
        }

        None
    }

    fn eval_conditional_object_multi_prop_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        infer_props: &[(Atom, TypeParamInfo, bool)],
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let mut subst = TypeSubstitution::new();
        for &(prop_name, info, optional) in infer_props {
            let Some(mut inferred) =
                self.resolve_conditional_infer_property(check_unwrapped, prop_name, optional)
            else {
                return self.evaluate(cond.false_type);
            };

            subst.insert(info.name, inferred);

            if let Some(constraint) = info.constraint {
                let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
                checker.allow_bivariant_rest = true;
                let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
                if optional {
                    let Some(filtered) =
                        self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                    else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                } else if is_union || cond.is_distributive {
                    inferred = self.filter_inferred_by_constraint_or_undefined(
                        inferred,
                        constraint,
                        &mut checker,
                    );
                } else if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
                subst.insert(info.name, inferred);
            }
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Handle object property infer pattern
    fn eval_conditional_object_prop_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        prop_name: tsz_common::interner::Atom,
        info: TypeParamInfo,
        prop_optional: bool,
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred =
            self.resolve_conditional_infer_property(check_unwrapped, prop_name, prop_optional);

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
            if prop_optional {
                let Some(filtered) =
                    self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                else {
                    let false_inst =
                        instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                    return self.evaluate(false_inst);
                };
                inferred = filtered;
            } else if is_union || cond.is_distributive {
                // For unions or distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For non-distributive single values, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    fn resolve_conditional_infer_property(
        &mut self,
        source: TypeId,
        prop_name: Atom,
        optional: bool,
    ) -> Option<TypeId> {
        if let Some(query_db) = self.query_db() {
            let prop_name_str = self.interner().resolve_atom_ref(prop_name);
            return match query_db.resolve_property_access(source, &prop_name_str) {
                PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                PropertyAccessResult::PropertyNotFound { .. } => {
                    optional.then_some(TypeId::UNDEFINED)
                }
                _ => None,
            };
        }

        match self.interner().lookup(source) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == prop_name)
                    .map(|prop| {
                        if optional {
                            self.optional_property_type(prop)
                        } else {
                            prop.type_id
                        }
                    })
                    .or_else(|| optional.then_some(TypeId::UNDEFINED))
            }
            Some(TypeData::Callable(callable_id)) => {
                // Callable types (class constructors) have static properties
                // that should participate in conditional infer resolution.
                // E.g., `typeof MyClass extends { defaultProps: infer D }` should
                // find `defaultProps` in the class constructor's static properties.
                let shape = self.interner().callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == prop_name)
                    .map(|prop| {
                        if optional {
                            self.optional_property_type(prop)
                        } else {
                            prop.type_id
                        }
                    })
                    .or_else(|| optional.then_some(TypeId::UNDEFINED))
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    let inferred = self.resolve_conditional_infer_property(
                        member_unwrapped,
                        prop_name,
                        optional,
                    )?;
                    inferred_members.push(inferred);
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union_from_slice(&inferred_members))
                }
            }
            Some(TypeData::Intersection(members)) => {
                // For intersection types (e.g., constructor & static props),
                // search each member for the property. The first member that has
                // the property wins (intersections share properties).
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    if let Some(inferred) = self.resolve_conditional_infer_property(
                        member_unwrapped,
                        prop_name,
                        optional,
                    ) {
                        return Some(inferred);
                    }
                }
                optional.then_some(TypeId::UNDEFINED)
            }
            _ => {
                // Fallback: try evaluating the source further and recursing.
                // This handles cases where the source is a TypeQuery, Lazy, Application
                // or other form that hasn't been fully evaluated.
                let evaluated = self.evaluate(source);
                if evaluated != source {
                    self.resolve_conditional_infer_property(evaluated, prop_name, optional)
                } else {
                    None
                }
            }
        }
    }

    /// Handle nested object infer pattern
    fn eval_conditional_object_nested_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        outer_name: tsz_common::interner::Atom,
        inner_name: tsz_common::interner::Atom,
        info: TypeParamInfo,
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred = match check_key {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == outer_name)
                    .and_then(|prop| {
                        let inner_type = match self.interner().lookup(prop.type_id) {
                            Some(TypeData::ReadonlyType(inner)) => inner,
                            _ => prop.type_id,
                        };
                        match self.interner().lookup(inner_type) {
                            Some(
                                TypeData::Object(inner_shape_id)
                                | TypeData::ObjectWithIndex(inner_shape_id),
                            ) => {
                                let inner_shape = self.interner().object_shape(inner_shape_id);
                                inner_shape
                                    .properties
                                    .iter()
                                    .find(|prop| prop.name == inner_name)
                                    .map(|prop| prop.type_id)
                            }
                            _ => None,
                        }
                    })
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                        self.interner().lookup(member_unwrapped)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let shape = self.interner().object_shape(shape_id);
                    let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, outer_name)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let inner_type = match self.interner().lookup(prop.type_id) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => prop.type_id,
                    };
                    let Some(
                        TypeData::Object(inner_shape_id)
                        | TypeData::ObjectWithIndex(inner_shape_id),
                    ) = self.interner().lookup(inner_type)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let inner_shape = self.interner().object_shape(inner_shape_id);
                    let Some(inner_prop) = inner_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == inner_name)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    inferred_members.push(inner_prop.type_id);
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union_from_slice(&inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
            if is_union || cond.is_distributive {
                // For unions or distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For non-distributive single values, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Try to match conditional types at the Application level before structural expansion.
    ///
    /// When both `check_type` and `extends_type` are Applications with the same base type
    /// (e.g., `Promise<string>` vs `Promise<infer U>`), we can match type arguments
    /// directly without expanding the interface structure. This is critical for complex
    /// generic interfaces like Promise, Map, Set where structural expansion makes the
    /// infer pattern matching fail.
    fn try_application_infer_match(&mut self, cond: &ConditionalType) -> Option<TypeId> {
        // Only proceed if extends_type is an Application containing infer.
        // Keep extends_type as-is (unevaluated) so match_infer_pattern can handle
        // it at the Application level. This is critical for complex generic interfaces
        // like Promise, Map, Set where structural expansion loses the ability to
        // match type arguments directly.
        let Some(TypeData::Application(_)) = self.interner().lookup(cond.extends_type) else {
            return None;
        };

        if !self.type_contains_infer(cond.extends_type) {
            return None;
        }

        // Use the raw (unevaluated) check_type — it may still be an Application
        // which enables Application-vs-Application matching in match_infer_pattern.
        // When the raw form is *not* an Application (e.g. an IndexAccess inside a
        // mapped-type per-key conditional like `S[K] extends Pattern<infer T>`),
        // evaluate it once: if evaluation yields an Application, that Application
        // is what we want to feed to `match_infer_pattern` so the
        // Application-vs-Application path can bind the infer arguments. Without
        // this, downstream `try_expand_application_for_conditional_check`
        // unfolds the evaluated Application into its structural Object form and
        // the Application-level match is irretrievably lost.
        // The raw `cond.check_type` may not be an Application (e.g. an
        // `IndexAccess` like `S[K]` inside a mapped-type per-key conditional).
        // Try to recover an Application form so the Application-vs-Application
        // path in `match_infer_pattern` can bind the infer arguments:
        //   1. Evaluate the raw type once. If that yields an Application,
        //      use it directly.
        //   2. Otherwise, the raw type may have evaluated to the *body* of
        //      an Application (the structural Object the body interned to).
        //      The interner records `display_alias[body] = Application` for
        //      every evaluated Application; consult it to recover the
        //      original Application form when the evaluated check_type is
        //      not itself an Application but came from one.
        let mut check_type = cond.check_type;
        if !matches!(
            self.interner().lookup(check_type),
            Some(TypeData::Application(_))
        ) {
            let evaluated = self.evaluate(check_type);
            if matches!(
                self.interner().lookup(evaluated),
                Some(TypeData::Application(_))
            ) {
                check_type = evaluated;
            } else if let Some(application_origin) = self.interner().get_display_alias(evaluated)
                && matches!(
                    self.interner().lookup(application_origin),
                    Some(TypeData::Application(_))
                )
            {
                check_type = application_origin;
            }
        }

        // Skip for special types
        if check_type == TypeId::ANY || check_type == TypeId::NEVER {
            return None;
        }
        if matches!(
            self.interner().lookup(check_type),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }

        // Try infer pattern matching with unevaluated types.
        // match_infer_pattern handles Application vs Application matching
        // by comparing base types and recursing on type arguments.
        let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
        checker.allow_bivariant_rest = true;
        let mut bindings = FxHashMap::default();
        let mut visited = FxHashSet::default();
        let matched = self.match_infer_pattern(
            check_type,
            cond.extends_type,
            &mut bindings,
            &mut visited,
            &mut checker,
        );
        if matched && !bindings.is_empty() {
            let substituted_true = self.substitute_infer(cond.true_type, &bindings);
            return Some(self.evaluate(substituted_true));
        }

        None
    }

    /// Check whether a type is an **intersection** of type parameters/Lazy refs.
    ///
    /// TSC defers conditional types when the check type is a naked type parameter.
    /// An intersection like `T & U` is NOT a naked type parameter (so Step 2 misses it),
    /// but the subtype relationship `T & U extends X` IS genuinely indeterminate until
    /// T and U are instantiated. This helper detects that case.
    ///
    /// We intentionally limit this to Intersection types. Other compound types like
    /// `keyof T`, `T[K]`, or `Lowercase<T>` are evaluated eagerly by TSC through
    /// constraint resolution and should NOT be deferred at this stage.
    ///
    /// Type parameters inside generic function bodies are represented as `Lazy(DefId)`
    /// references. The standard `contains_type_parameters` visitor doesn't walk through
    /// `Lazy` refs, so this helper checks for Lazy members directly.
    fn type_is_compound_generic(&self, type_id: TypeId) -> bool {
        // Check for compound types containing unresolved type parameter references.
        // We intentionally skip the `contains_type_parameters` visitor here because
        // it catches KeyOf(TypeParam), StringIntrinsic(_, TypeParam), etc., which
        // TSC evaluates eagerly via constraint resolution (not deferral).
        //
        // We handle two compound forms that TSC considers "generic" and defers:
        // - Intersections like `T & U` with type-parameter-like members
        // - IndexAccess like `T[K]` where object or index is generic
        //   (TSC's `isGenericType` returns true for IndexedAccessType with
        //   generic components, causing conditional type deferral)
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner().lookup(type_id) {
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner().type_list(list_id);
                members.iter().any(|&m| {
                    matches!(
                        self.interner().lookup(m),
                        Some(
                            TypeData::Lazy(_) | TypeData::Recursive(_) | TypeData::TypeParameter(_)
                        )
                    )
                })
            }
            Some(TypeData::IndexAccess(obj, idx)) => {
                // IndexAccess types like T[K] where T or K contains unresolved
                // type parameters (Lazy/TypeParameter) are genuinely indeterminate.
                // Example: Extract<M[K], ArrayLike<any>> must stay deferred because
                // M[K] could resolve to anything once M and K are instantiated.
                Self::is_generic_ref(self.interner(), obj)
                    || Self::is_generic_ref(self.interner(), idx)
            }
            _ => false,
        }
    }

    /// Check if a type is or contains a generic reference (Lazy/TypeParameter/Recursive).
    /// Recurses into nested `IndexAccess` to handle cases like `T[K1][K2]`.
    fn is_generic_ref(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match db.lookup(type_id) {
            Some(TypeData::Lazy(_) | TypeData::TypeParameter(_) | TypeData::Recursive(_)) => true,
            Some(TypeData::IndexAccess(obj, idx)) => {
                Self::is_generic_ref(db, obj) || Self::is_generic_ref(db, idx)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::TypeId;

    #[test]
    fn test_is_primitive_vs_function_intrinsic() {
        let interner = TypeInterner::new();
        // Primitives should match against TypeId::FUNCTION
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                TypeId::FUNCTION
            )
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::NUMBER,
                TypeId::FUNCTION
            )
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::BOOLEAN,
                TypeId::FUNCTION
            )
        );
        // Non-primitives should not match
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::OBJECT,
                TypeId::FUNCTION
            )
        );
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::ANY,
                TypeId::FUNCTION
            )
        );
        // Primitives against non-Function target should not match
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                TypeId::OBJECT
            )
        );
    }

    #[test]
    fn test_is_primitive_vs_function_structural() {
        let interner = TypeInterner::new();
        // Create an ObjectShape that looks like Function (has apply, call, bind)
        let apply = interner.intern_string("apply");
        let call = interner.intern_string("call");
        let bind = interner.intern_string("bind");
        let function_shape = interner.object(vec![
            crate::types::PropertyInfo {
                name: apply,
                type_id: TypeId::ANY,
                ..Default::default()
            },
            crate::types::PropertyInfo {
                name: call,
                type_id: TypeId::ANY,
                ..Default::default()
            },
            crate::types::PropertyInfo {
                name: bind,
                type_id: TypeId::ANY,
                ..Default::default()
            },
        ]);
        // string vs structural Function → should match
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                function_shape
            )
        );
        // Non-Function object → should not match
        let non_fn = interner.object(vec![crate::types::PropertyInfo {
            name: apply,
            type_id: TypeId::ANY,
            ..Default::default()
        }]);
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::is_primitive_vs_function(
                &interner,
                TypeId::STRING,
                non_fn
            )
        );
    }
}
