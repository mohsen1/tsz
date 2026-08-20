//! Core generic call resolution (`resolve_generic_call_inner`).

use super::visited::with_resolve_visited;
use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::debug;

use super::foreign_param_shapes::is_substantive_inference_candidate;
use super::readonly_direct_inference;
use super::{
    constraint_is_primitive_type_with_resolver, instantiate_call_type, type_implies_literals_deep,
    type_references_placeholder, write_placeholder_name,
};

mod constraint_helpers;
mod duplicate_shape;
mod finalize;
mod finalize_callbacks;
mod post_inference_helpers;

/// How each type-parameter inference variable is reached through the
/// context-sensitive callback parameters of a generic call, classified by the
/// variance of its occurrence (see `callback_position_inference_vars`).
struct CallbackPositionVars {
    /// Vars reached *only* through a callback **parameter** (contravariant)
    /// position, never a callback **return** position. Round-2 body inference
    /// cannot recover these from the callback body, so a contextual return type
    /// may pin them even when they also fill a direct-parameter slot.
    param_only: FxHashSet<InferenceVar>,
    /// Vars reached through any callback **return** (covariant) position. A
    /// Round-1 fix of such a var must not be widened by Round-2 callback-body
    /// inference — tsc's immutable `InferenceInfo.isFixed` (issue #17282).
    return_position: FxHashSet<InferenceVar>,
}

pub(super) struct FinishGenericCallResolutionArgs<'a> {
    pub(super) func: &'a FunctionShape,
    pub(super) arg_types: &'a [TypeId],
    pub(super) actual_this_type: Option<TypeId>,
    pub(super) infer_ctx: InferenceContext<'a>,
    pub(super) substitution: &'a TypeSubstitution,
    pub(super) type_param_vars: &'a [InferenceVar],
    pub(super) type_param_placeholder_atoms: &'a [tsz_common::Atom],
    pub(super) var_map: &'a FxHashMap<TypeId, InferenceVar>,
    pub(super) direct_param_vars: &'a FxHashSet<InferenceVar>,
    /// Placeholder-only substitution (type-param name -> fresh placeholder) used
    /// to classify callback type-parameter positions during finalization.
    pub(super) callback_placeholder_subst: &'a TypeSubstitution,
    /// Raw Round-1 fixes (variable -> inferred type), captured before any
    /// contextual-return override. Finalization restores one when a callback
    /// *return*-position variable was only widened by a concrete Round-2
    /// callback-body candidate (tsc's immutable `isFixed`; issue #17282).
    pub(super) round1_fixed: &'a FxHashMap<InferenceVar, TypeId>,
    pub(super) noinfer_param_vars: &'a FxHashSet<InferenceVar>,
    pub(super) rest_tuple_target_type: Option<TypeId>,
    /// Inference variables owned by the variadic element(s) of the aggregate
    /// rest tuple matched at this call.
    pub(super) aggregate_rest_inference_vars: &'a [InferenceVar],
    pub(super) structural_return_subst: &'a TypeSubstitution,
    pub(super) first_direct_primitive_mismatch: Option<(usize, TypeId, TypeId)>,
    pub(super) saw_deferred_arg: bool,
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn resolve_generic_call_inner(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        let _span = tracing::debug_span!(
            "resolve_generic_call",
            params = func.params.len(),
            args = arg_types.len(),
            type_params = func.type_params.len(),
        )
        .entered();

        let actual_this_type = self.actual_this_type;
        let has_context_sensitive_args = arg_types
            .iter()
            .copied()
            .any(|arg| self.is_contextually_sensitive(arg));

        // tsc's `inference.isFixed` set (recomputed at the finalize site). #17710.
        let contextually_fixed = self.contextually_fixed_type_params(func, arg_types);
        // Check argument count BEFORE type inference
        // This prevents false positive TS2554 errors for generic functions with optional/rest params
        let (min_args, max_args) = self.arg_count_bounds(&func.params, &func.type_params);

        if arg_types.len() < min_args {
            if let Some(result) = self.rest_tuple_mismatch_for_too_few_args(
                &func.params,
                &func.type_params,
                arg_types,
                func.return_type,
            ) {
                return result;
            }
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: max_args,
                actual: arg_types.len(),
            };
        }

        if let Some(max) = max_args
            && arg_types.len() > max
        {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: Some(max),
                actual: arg_types.len(),
            };
        }

        if let Some(result) = self.resolve_trivial_single_type_param_call(func, arg_types) {
            return result;
        }

        // #14344 / #14345 HKT-reduce lever (default-OFF, byte-parity): see
        // `build_inference_hkt_reduce_shim`. Declared before `infer_ctx` so the
        // shim outlives the inference context that borrows it.
        let hkt_reduce_shim = self.build_inference_hkt_reduce_shim();
        let mut infer_ctx = InferenceContext::with_query_db(self.interner);
        if let Some(ref shim) = hkt_reduce_shim {
            infer_ctx.resolver = Some(shim);
        }
        let mut substitution = TypeSubstitution::new();
        substitution.protect_type_parameters(&func.type_params);
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());
        // Store the placeholder atom for each type param var so we can look them
        // up later (e.g., to build fixed_subst after Round 1).  Indexed in
        // parallel with type_param_vars.
        let mut type_param_placeholder_atoms: Vec<tsz_common::Atom> =
            Vec::with_capacity(func.type_params.len());
        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        self.constraint_recursion_depth.set(0);
        self.constraint_step_count.set(0);

        // Reusable visited set for type_contains_placeholder checks (avoids per-iteration alloc)
        let mut placeholder_visited = FxHashSet::default();
        // Track placeholders that are used directly as argument targets.
        // For those parameters we keep inference constrained so final argument checks
        // can report concrete mismatches instead of silently widening to unions.
        let mut direct_param_vars = FxHashSet::default();
        let mut first_direct_primitive_candidate: FxHashMap<InferenceVar, TypeId> =
            FxHashMap::default();
        let mut first_direct_primitive_mismatch: Option<(usize, TypeId, TypeId)> = None;
        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
        let mut deferred_generic_function_arg_indices = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            // Allocate an inference variable first, then create a *unique* placeholder type
            // for that variable. We register the placeholder name (not the original type
            // parameter name) with the inference context so occurs-checks don't get confused
            // by identically-named type parameters from outer scopes (e.g., `T` inside `T`).
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            // Unique, deterministic placeholder type for this inference variable,
            // tracked by name during constraint collection (see naming helpers).
            let placeholder_id = self.checker.next_inference_placeholder_id();
            write_placeholder_name(&mut placeholder_buf, placeholder_id);
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            // Record the declared name -> var mapping so the inference engine can
            // recognize a self-referential inference (the declared parameter
            // leaking back into its own variable, e.g. through a callback
            // parameter contextually typed with the un-instantiated signature)
            // and skip the no-information (contra-)candidate it would otherwise add.
            infer_ctx.register_original_type_param_name(tp.name, var);
            let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
                origin: crate::types::TypeParamOrigin::InferPlaceholder { id: placeholder_id },
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);
            type_param_placeholder_atoms.push(placeholder_atom);

            // Add the type parameter constraint as an upper bound, but only if the
            // constraint is concrete (doesn't reference other type params via placeholders).
            // Constraints like `keyof T` that depend on other type params can't be evaluated
            // during resolution since T may not be resolved yet. These are validated in the
            // post-resolution constraint check below.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_call_type(
                    self.interner,
                    constraint,
                    &substitution,
                    actual_this_type,
                );
                placeholder_visited.clear();
                if !self.type_contains_placeholder(
                    inst_constraint,
                    &var_map,
                    &mut placeholder_visited,
                ) {
                    let resolver = self
                        .checker
                        .type_resolver()
                        .unwrap_or_else(|| self.interner.as_type_resolver());
                    if constraint_is_primitive_type_with_resolver(
                        self.interner,
                        resolver,
                        inst_constraint,
                    ) {
                        infer_ctx.mark_declared_constraint_preserves_literals(var);
                    }
                    infer_ctx.add_upper_bound(var, inst_constraint);
                    infer_ctx.set_declared_constraint(var, inst_constraint);
                }
            }

            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }

            // Mirror tsc's `widenLiteralTypes` gate: a fresh literal candidate for
            // a type parameter at the top level of the return type is not widened
            // when the parameter is unfixed / contextually pinned / conditional-
            // reducing. See `type_param_preserves_inferred_literal`.
            if self.type_param_preserves_inferred_literal(
                func,
                tp.name,
                contextually_fixed.as_ref(),
            ) {
                infer_ctx.mark_top_level_in_return_type_unfixed(var);
            }
        }

        // Record this call's inference placeholders and the subset shared across
        // more than one parameter. Higher-order (TS 3.4) re-generalization of a
        // generic function argument is only safe when the argument's contextual
        // placeholders are not chained through a shared inference variable (the
        // `B` of `compose(f: (a: A) => B, g: (b: B) => C)`). See
        // `instantiate_generic_function_argument_against_target`.
        //
        // This metadata is only consulted when an argument is itself a generic
        // function (the sole trigger for re-generalization), so the whole
        // computation — including the per-parameter `collect_referenced_types`
        // walk, which is expensive for large generic signatures — is skipped
        // when no argument is a generic function. The two sets are still cleared
        // so a later read cannot observe a previous call's state.
        self.current_call_inference_placeholders.clear();
        self.shared_inference_placeholders.clear();
        let any_arg_is_generic_function = arg_types.iter().any(|&arg_type| {
            Self::get_contextual_signature_cached(self.interner, arg_type)
                .is_some_and(|shape| !shape.type_params.is_empty())
        });
        if any_arg_is_generic_function {
            self.current_call_inference_placeholders
                .extend(type_param_placeholder_atoms.iter().copied());
            if func.params.len() > 1 {
                let mut param_reference_counts: FxHashMap<tsz_common::Atom, u32> =
                    FxHashMap::default();
                let mut seen_in_param: FxHashSet<tsz_common::Atom> = FxHashSet::default();
                for param in &func.params {
                    seen_in_param.clear();
                    for referenced in crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        param.type_id,
                    ) {
                        if let Some(info) =
                            crate::type_param_info(self.interner.as_type_database(), referenced)
                            && let Some(type_param) = func
                                .type_params
                                .iter()
                                .find(|type_param| type_param.is_same_binder(info))
                            && seen_in_param.insert(type_param.name)
                        {
                            *param_reference_counts.entry(type_param.name).or_insert(0) += 1;
                        }
                    }
                }
                for (tp, &placeholder_atom) in func
                    .type_params
                    .iter()
                    .zip(type_param_placeholder_atoms.iter())
                {
                    if param_reference_counts.get(&tp.name).copied().unwrap_or(0) > 1 {
                        self.shared_inference_placeholders.insert(placeholder_atom);
                    }
                }
            }
        }

        // Re-set declared constraints using the full substitution now that all
        // placeholders exist. When type parameter order is `<T extends ..U.., U extends P>`,
        // the initial pass above sets T's constraint before U's placeholder exists, so
        // the constraint may contain the original (unconstrained) TypeParameter for U.
        // Re-instantiating with the complete substitution replaces those stale references
        // with U's placeholder, which carries U's constraint in its TypeParamInfo.
        // This is critical for `constraint_contains_type_param_with_primitive_constraint`
        // which checks the constraint of TypeParameters found inside T's constraint
        // (e.g., Object.freeze: T extends {[idx:string]: U|...}, U extends string|...).
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_call_type(
                    self.interner,
                    constraint,
                    &substitution,
                    actual_this_type,
                );
                let resolver = self
                    .checker
                    .type_resolver()
                    .unwrap_or_else(|| self.interner.as_type_resolver());
                if constraint_is_primitive_type_with_resolver(
                    self.interner,
                    resolver,
                    inst_constraint,
                ) {
                    infer_ctx.mark_declared_constraint_preserves_literals(var);
                }
                infer_ctx.set_declared_constraint(var, inst_constraint);
            }
        }

        // Record the implied arity for a non-array rest type parameter (tsc's
        // `getNonArrayRestType` path). For a signature whose rest parameter is a
        // bare type parameter (`...rest: T`), the number of trailing arguments
        // that land in the rest parameter is the implied arity of `T`. Variadic
        // tuple inference uses it to split a `[...A, ...B]` target so the tail
        // type parameter keeps its arity (e.g. partial-application / `bind`).
        self.record_rest_param_implied_arity(&mut infer_ctx, func, arg_types, &type_param_vars);

        // Seed inference from generic `this` parameter when present.
        // For calls like `obj.method<T>(...)`, `this: T` must constrain `T` from
        // the calling receiver so parameter types like `keyof T` don't collapse.
        if let Some(expected_this) = func.this_type {
            let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
            let expected_this_inst = instantiate_call_type(
                self.interner,
                expected_this,
                &substitution,
                actual_this_type,
            );
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                actual_this,
                expected_this_inst,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }

        // 1.5. Pre-compute which placeholders should have their argument's object
        // properties widened. In tsc, object literal property widening happens at the
        // expression level (checkObjectLiteral) based on contextual type. When the
        // contextual type is a bare type parameter whose constraint doesn't contain
        // literal types, properties like `false` are widened to `boolean`.
        //
        // We suppress widening in three cases:
        // (a) The constraint contains literal types (discriminated union protection).
        // (b) The placeholder is referenced in another type param's constraint,
        //     because widening would cause a mismatch between the widened candidate
        //     and the un-widened contextual type used for callback parameters.
        // (c) The type parameter has the TS 5.0 `const` modifier and its constraint
        //     does not allow a mutable array-like target. `const T` preserves the
        //     literal shape of the argument expression, so the round-1 inference
        //     seed must be the un-widened argument shape — without this,
        //     `<const T>(x: T, y: number)` widens `{ a: 1 }` to `{ a: number }`
        //     before inference and the literal is lost.
        let widenable_placeholders: FxHashSet<TypeId> = var_map
            .keys()
            .filter(|&&placeholder_id| {
                let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(placeholder_id) else {
                    return false;
                };
                // Instantiate the placeholder's own constraint once and share it
                // between (a) literal-implication and (c) const-mutable-array checks.
                let inst_constraint = tp.constraint.map(|constraint| {
                    instantiate_call_type(
                        self.interner,
                        constraint,
                        &substitution,
                        actual_this_type,
                    )
                });
                // (a)
                if inst_constraint
                    .is_some_and(|inst| type_implies_literals_deep(self.interner, inst))
                {
                    return false;
                }
                // (b)
                let own_type_param = var_map.get(&placeholder_id).and_then(|own_var| {
                    func.type_params
                        .iter()
                        .zip(type_param_vars.iter())
                        .find_map(|(type_param, candidate_var)| {
                            (candidate_var == own_var).then_some(*type_param)
                        })
                });
                let is_referenced_in_other_constraints = func.type_params.iter().any(|other_tp| {
                    if own_type_param.is_some_and(|own| other_tp.is_same_binder(own)) {
                        return false;
                    }
                    let Some(constraint) = other_tp.constraint else {
                        return false;
                    };
                    let inst = instantiate_call_type(
                        self.interner,
                        constraint,
                        &substitution,
                        actual_this_type,
                    );
                    type_references_placeholder(self.interner, inst, placeholder_id)
                });
                if is_referenced_in_other_constraints {
                    return false;
                }
                // (c). An unconstrained `const T` falls through here: no constraint
                // means no mutable-array-like target, so widening is suppressed —
                // which matches tsc's behavior of preserving literals for `const T`.
                if tp.is_const
                    && !inst_constraint.is_some_and(|inst| {
                        crate::type_queries::constraint_allows_mutable_array_like(
                            self.interner,
                            inst,
                        )
                    })
                {
                    return false;
                }
                true
            })
            .copied()
            .collect();

        // 2. Instantiate parameters with placeholders
        let mut instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated = instantiate_call_type(
                    self.interner,
                    p.type_id,
                    &substitution,
                    actual_this_type,
                );
                if let Some(name_atom) = p.name {
                    let param_name = self.interner.resolve_atom(name_atom);
                    debug!(
                        param_name = %param_name.as_str(),
                        original_type_id = p.type_id.0,
                        original_type_key = ?self.interner.lookup(p.type_id),
                        instantiated_type_id = instantiated.0,
                        instantiated_type_key = ?self.interner.lookup(instantiated),
                        "Instantiated param"
                    );
                }
                // If this is a function type, also log its return type
                if let Some(TypeData::Function(shape_id)) = self.interner.lookup(instantiated) {
                    let shape = self.interner.function_shape(shape_id);
                    debug!(
                        return_type_id = shape.return_type.0,
                        return_type_key = ?self.interner.lookup(shape.return_type),
                        "Instantiated function return type"
                    );
                }
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();

        // Snapshot the placeholder-only substitution before any contextual-return
        // pre-substitution mutates it; finalization uses it to classify callback
        // type-parameter positions without perturbing in-flight inference.
        let callback_placeholder_subst = substitution.clone();

        let mut noinfer_param_vars = FxHashSet::default();
        for param in &instantiated_params {
            placeholder_visited.clear();
            self.collect_noinfer_placeholder_vars_in_type(
                param.type_id,
                &var_map,
                &mut noinfer_param_vars,
                &mut placeholder_probe_map,
                &mut placeholder_visited,
            );
        }

        // Track bare return type placeholder for conditional seeding after Round 1
        let mut return_type_bare_var: Option<(crate::inference::infer::InferenceVar, TypeId)> =
            None;
        // Name of a bare return type parameter that is also directly pinned by a
        // concrete value argument (#14262). For such a parameter argument
        // inference owns the result, so the contextual-return type must stay a
        // low-priority hint: its return-context substitution is dropped below so
        // it cannot clamp the argument-pinned callback/return.
        let mut value_arg_seeded_bare_return_param: Option<tsz_common::Atom> = None;
        let mut round1_direct_seed_vars = FxHashSet::default();
        let mut pair_visited = FxHashSet::default();

        for (i, &arg_type) in arg_types.iter().enumerate() {
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
            if self.arg_targets_aggregate_rest_param(&instantiated_params, i, arg_type) {
                continue;
            }
            if self
                .contextual_round1_arg_types(arg_type, target_type)
                .is_some()
            {
                if self.is_contextually_sensitive(arg_type) {
                    // Context-sensitive arguments resolve in Round 2; keep the
                    // structural estimate of which return-type vars they cover.
                    round1_direct_seed_vars.extend(self.collect_placeholder_vars_in_type(
                        target_type,
                        &var_map,
                        &mut placeholder_probe_map,
                        &mut placeholder_visited,
                    ));
                } else {
                    // A non-sensitive argument only constrains placeholders its
                    // own structure reaches. Counting every placeholder in the
                    // parameter type falsely marked vars behind omitted optional
                    // members as covered, skipping contextual-return seeding and
                    // leaving those type parameters at `unknown` (#14171).
                    pair_visited.clear();
                    self.collect_round1_reachable_placeholder_vars(
                        arg_type,
                        target_type,
                        &var_map,
                        &mut pair_visited,
                        &mut round1_direct_seed_vars,
                    );
                }
            }
        }

        // 2.5. Seed contextual constraints from return type BEFORE argument processing
        // This enables downward inference: `let x: string = id(...)` should infer T = string
        // Contextual hints use lower priority so explicit arguments can override
        // Skip `any` and `unknown` contextual types — they carry no inference information
        // and can interfere with constraint-based inference (e.g., causing T to resolve to
        // `any` instead of using its constraint like `(arg: string) => any`).
        if let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders = instantiate_call_type(
                self.interner,
                func.return_type,
                &substitution,
                actual_this_type,
            );
            let return_seed_vars = self.collect_placeholder_vars_in_type(
                return_type_with_placeholders,
                &var_map,
                &mut placeholder_probe_map,
                &mut placeholder_visited,
            );
            // Skip contextual return type seeding only when ALL return type vars
            // are already covered by round-1 (direct argument) inference AND the
            // return type is not a bare type parameter. When the return type is a
            // bare type parameter (e.g., `<T>(f: () => T): T`), the contextual type
            // provides a critical upper bound that prevents literal widening.
            // Without this, `let x: 0|1|2 = invoke(() => 1)` would widen T to
            // `number` because the contextual `0|1|2` upper bound is never set.
            let return_is_bare_var = var_map.contains_key(&return_type_with_placeholders);
            // When the return type is a bare type parameter `T` that is ALSO
            // directly seeded by a concrete value argument (some parameter's
            // type IS `T` and the corresponding argument is not a deferred,
            // context-sensitive expression), argument inference owns `T`. tsc
            // treats the contextual-return type — including an outer `as`-cast
            // target like `as never` — as a low-priority `ReturnType` hint that
            // cannot override the argument-pinned inference (#14262).
            //
            // We detect the *naked* value-parameter position (`init: T`), not
            // any reachable occurrence, so the literal-widening upper bound is
            // preserved for callback-return positions like `<T>(f: () => T): T`
            // (`let x: 0|1|2 = invoke(() => 1)`), where `T` is only seeded
            // through the callback's return type and the contextual type is the
            // sole anchor that prevents widening. The matched type parameter's
            // name is recorded so its return-context substitution is dropped
            // below (it must not clamp the callback parameters / return type).
            if let Some(rv) = var_map.get(&return_type_with_placeholders).copied() {
                let pinned_by_value_arg = arg_types.iter().enumerate().any(|(i, &arg_type)| {
                    self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
                        .is_some_and(|param_type| var_map.get(&param_type).copied() == Some(rv))
                        && !self.is_contextually_sensitive(arg_type)
                });
                if pinned_by_value_arg {
                    value_arg_seeded_bare_return_param = func
                        .type_params
                        .iter()
                        .zip(type_param_vars.iter())
                        .find(|(_, v)| **v == rv)
                        .map(|(tp, _)| tp.name);
                }
            }
            let bare_return_var_value_arg_seeded = value_arg_seeded_bare_return_param.is_some();
            // When the contextual type is a generic function (has type parameters),
            // always seed from it regardless of coverage. Higher-order generic
            // patterns like `compose(list, box)` need the contextual type's
            // TypeParameters (e.g., T from `<T>(x: T) => Box<T[]>`) to flow into
            // the inference — argument processing alone only establishes
            // inter-placeholder relationships without concrete type anchors.
            let contextual_is_generic_function =
                crate::type_queries::get_function_shape(self.interner.as_type_database(), ctx_type)
                    .is_some_and(|shape| !shape.type_params.is_empty());
            let all_return_vars_covered = !return_is_bare_var
                && !contextual_is_generic_function
                && !return_seed_vars.is_empty()
                && return_seed_vars
                    .iter()
                    .all(|var| round1_direct_seed_vars.contains(var));
            if !all_return_vars_covered {
                // When the return type is a union containing a placeholder
                // (like `E | null`), use tsc-compatible reversed direction so
                // that the union target handler can extract the placeholder
                // member and add the contextual type as a candidate. This
                // matches tsc's inferTypes(contextualType, returnType).
                // For non-union return types (bare `T` or structural types),
                // keep the original direction to preserve upper-bound
                // semantics and avoid interfering with argument inference
                // (e.g., foo<T>(x: T, y: T): T where arguments already
                // provide NakedTypeVariable candidates for T).
                let return_is_union_with_placeholder = matches!(
                    self.interner.lookup(return_type_with_placeholders),
                    Some(TypeData::Union(_))
                ) && with_resolve_visited(|visited| {
                    self.type_contains_placeholder(return_type_with_placeholders, &var_map, visited)
                });
                // Construct signatures whose declared parameters reference
                // NONE of the signature's type parameters also use the
                // reversed (tsc) direction: `new C(...)` with a contextual
                // instance type runs
                // inferTypes(contextualType, instanceTypeWithPlaceholders), so
                // structural matching (heritage members, same-base nested
                // applications) records CANDIDATES for the class type
                // parameters. The original direction only produces upper
                // bounds, leaving parameters that appear solely in member
                // signatures unconstrained — they then fall back to
                // constraint/`unknown` even though the contextual type
                // determines them (e.g. `class Impl<A,B> implements I<A,B>`
                // with a non-generic constructor, returned where `I<A,B>` is
                // expected). When any parameter mentions a type parameter
                // (placeholder instantiation changed it, e.g. React's
                // `new (props: P) => Component<P, S>`), argument inference
                // owns those variables and the original direction is kept.
                let constructor_params_lack_type_params =
                    func.is_constructor
                        && func.params.iter().zip(instantiated_params.iter()).all(
                            |(original, instantiated)| original.type_id == instantiated.type_id,
                        );
                // A construct signature can mention a class type parameter only
                // in a position round-1 argument inference does not actually pin
                // — most commonly a contravariant callback parameter such as
                // `refiner?: (value: T) => boolean`. There `constructor_params_lack_type_params`
                // is false (the param type changed under placeholder
                // instantiation), so the original direction would record only an
                // upper bound and the parameter falls back to `unknown` (#14822).
                // When such a return-type variable is genuinely uncovered by
                // round-1 direct seeding, use the reversed (tsc) direction so the
                // contextual instance type records it as a CANDIDATE. Covered
                // variables keep their stronger round-1 candidates, which win on
                // priority, so this does not disturb constructors whose value
                // parameters pin their type parameters (e.g. React's
                // `new (props: P) => Component<P, S>`).
                let constructor_has_uncovered_return_var = func.is_constructor
                    && return_seed_vars
                        .iter()
                        .any(|var| !round1_direct_seed_vars.contains(var));
                if bare_return_var_value_arg_seeded {
                    // Argument inference owns `T`: record the contextual type as a
                    // low-priority `ReturnType` candidate (a hint) rather than an
                    // upper bound. Argument candidates carry a higher priority
                    // (`NakedTypeVariable`) and win during candidate filtering, so
                    // an outer `as never` / `as SomeNarrower` cannot clamp `T`.
                    if let Some(&var) = var_map.get(&return_type_with_placeholders) {
                        infer_ctx.add_candidate(
                            var,
                            ctx_type,
                            crate::types::InferencePriority::ReturnType,
                        );
                    }
                } else if return_is_union_with_placeholder
                    || constructor_params_lack_type_params
                    || constructor_has_uncovered_return_var
                {
                    self.constrain_types(
                        &mut infer_ctx,
                        &var_map,
                        ctx_type,                      // source (contextual type)
                        return_type_with_placeholders, // target (union with vars)
                        crate::types::InferencePriority::ReturnType,
                    );
                } else {
                    self.constrain_types(
                        &mut infer_ctx,
                        &var_map,
                        return_type_with_placeholders, // source
                        ctx_type,                      // target
                        crate::types::InferencePriority::ReturnType,
                    );
                }

                // When the return type is an intersection (`{ ... } & D`) whose
                // only inference placeholder is a bare, return-only conjunct `D`,
                // the original-direction `constrain_types` above records only an
                // upper bound `D <: ctx`. If `D` carries a `D extends C = C`
                // default, resolution then treats "default + upper-bounds-only" as
                // "no inference happened" and falls back to the default, dropping
                // the contextual target (false TS2322/TS2741). tsc instead infers a
                // single naked type-variable conjunct of an intersection target
                // from the whole contextual source (`inferToMultipleTypes`,
                // `typeVariableCount === 1`). Seed the contextual type as a
                // `ReturnType` candidate (a lower bound) so the default applies
                // only when no contextual type exists. Restricted to exactly one
                // return-only naked conjunct with no other placeholder-bearing
                // conjunct, matching tsc's single-type-variable intersection rule.
                if let Some(TypeData::Intersection(members_id)) =
                    self.interner.lookup(return_type_with_placeholders)
                {
                    let members = self.interner.type_list(members_id);
                    let mut naked_return_only_var: Option<InferenceVar> = None;
                    let mut blocked = false;
                    for &member in members.iter() {
                        if let Some(&var) = var_map.get(&member) {
                            // A bare naked type-variable conjunct. Skip seeding when
                            // a direct argument already owns it, or when more than
                            // one naked conjunct exists (ambiguous, not tsc's
                            // single-type-variable case).
                            if round1_direct_seed_vars.contains(&var)
                                || naked_return_only_var.is_some()
                            {
                                blocked = true;
                                break;
                            }
                            naked_return_only_var = Some(var);
                        } else {
                            placeholder_visited.clear();
                            if self.type_contains_placeholder(
                                member,
                                &var_map,
                                &mut placeholder_visited,
                            ) {
                                // Another conjunct nests a placeholder; structural
                                // inference owns it, so leave the original direction.
                                blocked = true;
                                break;
                            }
                        }
                    }
                    if !blocked && let Some(var) = naked_return_only_var {
                        infer_ctx.add_candidate(
                            var,
                            ctx_type,
                            crate::types::InferencePriority::ReturnType,
                        );
                    }
                }

                // When the return type is a union containing a single placeholder
                // (e.g., `E | null`), the structural constrain_types adds the
                // contextual type as an upper bound for E. But for contextual return
                // type inference, E should get the contextual type (minus nullish
                // members) as a candidate (lower bound), matching tsc's behavior.
                // Without this, `querySelector<E>(): E | null` with contextual type
                // `MyElement` would resolve E to the default instead of MyElement.
                if let Some(TypeData::Union(members_id)) =
                    self.interner.lookup(return_type_with_placeholders)
                {
                    let members = self.interner.type_list(members_id);
                    // Collect non-placeholder, non-nullish target members for filtering
                    let ctx_stripped = if let Some(TypeData::Union(ctx_members_id)) =
                        self.interner.lookup(ctx_type)
                    {
                        let ctx_members = self.interner.type_list(ctx_members_id);
                        let non_nullish: Vec<TypeId> = ctx_members
                            .iter()
                            .copied()
                            .filter(|m| !m.is_nullable())
                            .collect();
                        if non_nullish.len() == 1 {
                            Some(non_nullish[0])
                        } else if non_nullish.is_empty() {
                            None
                        } else {
                            Some(self.interner.union(non_nullish))
                        }
                    } else if !ctx_type.is_nullable() {
                        Some(ctx_type)
                    } else {
                        None
                    };

                    if let Some(ctx_stripped) = ctx_stripped {
                        for &member in members.iter() {
                            if let Some(&var) = var_map.get(&member) {
                                infer_ctx.add_candidate(
                                    var,
                                    ctx_stripped,
                                    crate::types::InferencePriority::ReturnType,
                                );
                            }
                        }
                    }
                }

                // Track whether the return type is a bare type parameter placeholder.
                // If so, we may need to add a ReturnType candidate AFTER Round 1
                // (see below, before fix_current_variables).
                if let Some(&var) = var_map.get(&return_type_with_placeholders) {
                    return_type_bare_var = Some((var, ctx_type));
                }
                // Also handle union return types like `T | null` or `T | undefined`.
                // When the return type is a union containing a bare placeholder and
                // fixed members (null/undefined), extract the placeholder and match
                // it against the contextual type minus the corresponding fixed members.
                // This enables correct inference for patterns like:
                //   declare function f<T extends E = E>(): T | null;
                //   let x: HTMLElement | null = f(); // T should be HTMLElement
                //   let y: HTMLElement = f()!;       // T should be HTMLElement
                else if let Some(TypeData::Union(ret_members_id)) =
                    self.interner.lookup(return_type_with_placeholders)
                {
                    let ret_members = self.interner.type_list(ret_members_id);
                    // Find the single placeholder member in the return type union
                    let mut placeholder_var = None;
                    let mut fixed_ret_members = Vec::new();
                    for &member in ret_members.iter() {
                        if let Some(&var) = var_map.get(&member) {
                            if placeholder_var.is_none() {
                                placeholder_var = Some(var);
                            }
                        } else {
                            fixed_ret_members.push(member);
                        }
                    }
                    if let Some(var) = placeholder_var
                        && !fixed_ret_members.is_empty()
                    {
                        // Compute the effective contextual target for the placeholder
                        // by stripping fixed return type members from the contextual type
                        let effective_ctx = if let Some(TypeData::Union(ctx_members_id)) =
                            self.interner.lookup(ctx_type)
                        {
                            // Both are unions: strip matching fixed members
                            let ctx_members = self.interner.type_list(ctx_members_id);
                            let fixed_set: FxHashSet<TypeId> =
                                fixed_ret_members.iter().copied().collect();
                            let filtered_ctx: Vec<TypeId> = ctx_members
                                .iter()
                                .copied()
                                .filter(|t| !fixed_set.contains(t))
                                .collect();
                            if filtered_ctx.is_empty() {
                                None
                            } else if filtered_ctx.len() == 1 {
                                Some(filtered_ctx[0])
                            } else {
                                Some(self.interner.union(filtered_ctx))
                            }
                        } else {
                            // Contextual type is not a union (e.g., `HTMLElement`
                            // from `let x: HTMLElement = f()!`). The fixed return
                            // members (null/undefined) don't appear in the contextual
                            // type, so use the contextual type directly as the target.
                            Some(ctx_type)
                        };
                        if let Some(ctx) = effective_ctx {
                            return_type_bare_var = Some((var, ctx));
                        }
                    }
                }

                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    crate::types::InferencePriority::ReturnType,
                );
            }
        }

        let mut structural_return_subst =
            self.compute_return_context_substitution(func, self.contextual_type);
        // Drop the return-context substitution for a bare return type parameter
        // that a concrete value argument already pins (#14262). Argument
        // inference owns the parameter, so the contextual-return type (e.g. an
        // outer `as never`) must remain a low-priority hint and never reach the
        // callback parameters, return type, or fixed substitution as a clamp.
        if let Some(name) = value_arg_seeded_bare_return_param {
            structural_return_subst.remove(name);
        }
        // Add literal-containing upper bounds from the return context
        // substitution to prevent incorrect widening. When TResult1 has a
        // return context of DooDad = "SOMETHING" | "ELSE", the literal "ELSE"
        // from the callback should NOT be widened to string.
        if !structural_return_subst.is_empty() {
            for (&name, &ty) in structural_return_subst.map().iter() {
                if self.type_contains_literals(ty)
                    && let Some(tp_idx) = func.type_params.iter().position(|tp| tp.name == name)
                {
                    let var = type_param_vars[tp_idx];
                    infer_ctx.add_upper_bound(var, ty);
                }
            }
        }
        let has_structural_return_generic_function_args =
            arg_types.iter().copied().any(|arg_type| {
                Self::get_contextual_signature_cached(self.interner, arg_type).is_some_and(
                    |shape| {
                        !shape.type_params.is_empty()
                            && !matches!(
                                self.interner.lookup(shape.return_type),
                                Some(TypeData::TypeParameter(info))
                                    if shape
                                        .type_params
                                        .iter()
                                        .any(|type_param| type_param.is_same_binder(info))
                            )
                    },
                )
            });
        let contextual_type_is_non_generic_function = self.contextual_type.is_some_and(|ctx| {
            Self::get_contextual_signature_cached(self.interner, ctx)
                .is_some_and(|shape| shape.type_params.is_empty())
        });
        if (has_context_sensitive_args
            || ((arg_types.len() > 1 || contextual_type_is_non_generic_function)
                && has_structural_return_generic_function_args))
            && !structural_return_subst.is_empty()
        {
            for (&name, &ty) in structural_return_subst.map().iter() {
                substitution.insert(name, ty);
            }
            instantiated_params = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_call_type(
                        self.interner,
                        p.type_id,
                        &substitution,
                        actual_this_type,
                    ),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
        }

        // 3. Multi-pass constraint collection for proper contextual typing

        // Prepare rest tuple inference info
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _, _)| *start);
        let rest_tuple_target_type = rest_tuple_inference
            .as_ref()
            .map(|(_, target_type, _, _)| *target_type);
        let mut aggregate_rest_inference_vars = Vec::new();
        let mut saw_deferred_arg = false;
        // Track whether any deferred (context-sensitive) arg's target type
        // contains the return type bare var's placeholder. If so, Round 2 will
        // provide a better candidate for that var, and we should NOT seed from
        // the contextual return type.
        let mut deferred_arg_covers_return_var = false;

        // === Round 1: Process non-contextual arguments ===
        // These are arguments like arrays, primitives, and objects that don't need
        // contextual typing. Processing them first allows us to infer type parameters
        // that contextual arguments (lambdas) can then use.
        for (i, &arg_type) in arg_types.iter().enumerate() {
            // #17282: expose this argument's unannotated-callback-parameter mask
            // to the parameter constraint walk.
            self.current_arg_callback_param_unannotated = self
                .arg_callback_param_unannotated
                .get(i)
                .filter(|m| !m.is_empty())
                .cloned();
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
            if self.arg_targets_aggregate_rest_param(&instantiated_params, i, arg_type) {
                continue;
            }

            let target_type_param = var_map.get(&target_type).and_then(|&var| {
                func.type_params
                    .iter()
                    .zip(type_param_vars.iter())
                    .find_map(|(tp, candidate_var)| (*candidate_var == var).then_some(*tp))
            });

            // Defer a bare-type-parameter argument when a later context-sensitive
            // generic function-like argument depends on the same type parameter.
            // Non-context-sensitive arguments (primitives, variable references) must
            // NOT be deferred: Round 2 only processes context-sensitive arguments,
            // so deferring a non-context-sensitive arg loses its constraint entirely,
            // causing the type parameter to resolve to `unknown`.
            if let Some(type_param) = target_type_param
                && self.is_contextually_sensitive(arg_type)
                && self.later_generic_function_like_arg_depends_on_type_param(
                    func, arg_types, i, type_param,
                )
            {
                saw_deferred_arg = true;
                continue;
            }

            // Keep round-2 contextual arguments for full checking, but only process
            // non-contextual arguments (and non-contextual parts of mixed objects) in
            // round 1.
            let Some((contextual_arg_type, contextual_target_type)) =
                self.contextual_round1_arg_types(arg_type, target_type)
            else {
                saw_deferred_arg = true;
                // Check if this deferred arg is a concrete function (all non-`any`
                // params) whose target type references the return type bare var.
                // If so, Round 2 will get inference for that var from the concrete
                // function, and we should NOT pre-seed the var from the contextual
                // return type.
                //
                // We only suppress the seed for concrete functions — lambdas with
                // `any`-typed params genuinely need the var pre-fixed for contextual
                // typing.
                if !deferred_arg_covers_return_var && let Some((_, _)) = return_type_bare_var {
                    let is_concrete_function = match self.interner.lookup(arg_type) {
                        Some(TypeData::Function(shape_id)) => {
                            let shape = self.interner.function_shape(shape_id);
                            !shape.params.is_empty()
                                && shape.params.iter().all(|p| p.type_id != TypeId::ANY)
                        }
                        Some(TypeData::Callable(shape_id)) => {
                            let shape = self.interner.callable_shape(shape_id);
                            shape
                                .call_signatures
                                .iter()
                                .chain(shape.construct_signatures.iter())
                                .any(|sig| {
                                    !sig.params.is_empty()
                                        && sig.params.iter().all(|p| p.type_id != TypeId::ANY)
                                })
                        }
                        _ => false,
                    };
                    if is_concrete_function {
                        placeholder_visited.clear();
                        if self.type_contains_placeholder(
                            target_type,
                            &var_map,
                            &mut placeholder_visited,
                        ) {
                            deferred_arg_covers_return_var = true;
                        }
                    }
                }
                continue;
            };
            if self.is_contextually_sensitive(arg_type) {
                saw_deferred_arg = true;
            }
            let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                && i >= instantiated_params.len().saturating_sub(1);

            // When the checker contextually types an inline arrow using the union of
            // overload signatures, the arrow's parameter types may contain the original
            // (pre-substitution) type parameters from the caller's signature (e.g., `T`
            // from `map<T, U>(c: C<T>, f: (x: T) => U)`). These leaked type parameters
            // would create spurious constraints in Round 1, poisoning inference.
            // Defer such args to Round 2, where they will be re-typed with the specific
            // overload's contextual type after type parameters are resolved from Round 1.
            //
            // Only apply this check to contextually sensitive arguments — those whose
            // parameter types came from contextual typing. For fully annotated function
            // arguments (e.g., `(x: T) => ''` where `T` is from an outer scope), the
            // parameter types are explicit source annotations, not leaked caller type
            // params. Deferring them would cause both rounds to skip inference, since
            // Round 2 only processes contextually sensitive args.
            if self.is_contextually_sensitive(arg_type)
                && self.arg_contains_callers_type_params(contextual_arg_type, &substitution)
            {
                saw_deferred_arg = true;
                continue;
            }

            // Direct placeholders (inference variables) are validated by final
            // constraint resolution below. Skipping eager checks here avoids
            // duplicate expensive assignability work on hot generic-call paths.
            // Track rest parameter args for direct placeholder vars too.
            // In tsc, `foo<T>(...s: T[])` called with `foo(1, "hello")` uses
            // first-wins logic: T = 1, and "hello" fails with TS2345.
            // This also covers iterable spreads: `foo(...symbolIter, ...stringIter)`.
            let track_direct_placeholder_vars = !self.type_evaluates_to_function(target_type);

            if !var_map.contains_key(&target_type) {
                placeholder_visited.clear();
                if !self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited)
                {
                    // No placeholder in target_type - check assignability directly.
                    //
                    // An optional parameter (`?`) or one carrying a default
                    // initializer implicitly accepts `undefined` at the call site, so
                    // an argument of `T | undefined` is valid against a `T` parameter.
                    // The non-generic `check_argument_types_with` path strips
                    // `undefined` from such arguments before checking; mirror that here
                    // so the eager concrete-parameter check in generic inference does
                    // not falsely reject `ErrorConfig | undefined` against an optional
                    // `ErrorConfig` parameter.
                    let param_is_optional = instantiated_params
                        .get(i)
                        .or_else(|| {
                            let last = instantiated_params.last()?;
                            last.rest.then_some(last)
                        })
                        .is_some_and(|param| param.optional);
                    let arg_for_check = if param_is_optional {
                        crate::narrowing::utils::remove_undefined(
                            self.interner,
                            contextual_arg_type,
                        )
                    } else {
                        contextual_arg_type
                    };
                    if !self
                        .checker
                        .is_assignable_to(arg_for_check, contextual_target_type)
                        && !self.is_function_union_compat(arg_for_check, contextual_target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: contextual_target_type,
                            actual: contextual_arg_type,
                            fallback_return: TypeId::ERROR,
                        };
                    }
                    if track_direct_placeholder_vars
                        && let Some(direct_target) =
                            self.direct_inference_tracking_target(target_type)
                    {
                        placeholder_visited.clear();
                        direct_param_vars.extend(self.collect_direct_placeholder_vars_in_type(
                            direct_target,
                            &var_map,
                            &mut placeholder_visited,
                        ));
                    }
                } else {
                    // Target type contains placeholders - check against their constraints.
                    // Only track as "direct parameter" when the placeholder is NOT inside
                    // a union/intersection. When the parameter type is `T | string`, the
                    // inference decomposes the argument union and each non-matching member
                    // becomes a separate candidate for T. These candidates should be combined
                    // into a union, NOT reduced via first-wins logic. The first-wins behavior
                    // in `resolve_direct_parameter_inference_type` is designed for cases where
                    // T appears bare in multiple parameters (e.g., `f<T>(a: T, b: T)`) and
                    // heterogeneous arguments produce conflicting candidates.
                    if track_direct_placeholder_vars
                        && let Some(direct_target) =
                            self.direct_inference_tracking_target(contextual_target_type)
                    {
                        placeholder_visited.clear();
                        direct_param_vars.extend(self.collect_direct_placeholder_vars_in_type(
                            direct_target,
                            &var_map,
                            &mut placeholder_visited,
                        ));
                    }
                    // When the target type is a type parameter placeholder with a constraint,
                    // check if the argument is assignable to the constraint. If not,
                    // the call will fail after inference. Note: we only check constraints,
                    // not defaults, because defaults are fallback types when inference
                    // fails, not requirements for the argument.
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(target_type) {
                        // Only check constraint, not default
                        if let Some(check_type_id) = tp.constraint {
                            let inst_check_type = instantiate_call_type(
                                self.interner,
                                check_type_id,
                                &substitution,
                                actual_this_type,
                            );
                            placeholder_visited.clear();
                            if !self.type_contains_placeholder(
                                inst_check_type,
                                &var_map,
                                &mut placeholder_visited,
                            ) {
                                // Check type is fully concrete - safe to check now
                                if !self
                                    .checker
                                    .is_assignable_to(contextual_arg_type, inst_check_type)
                                    && !self.is_function_union_compat(
                                        contextual_arg_type,
                                        inst_check_type,
                                    )
                                    && !self.callable_satisfies_top_rest_any_constraint(
                                        contextual_arg_type,
                                        inst_check_type,
                                    )
                                {
                                    return CallResult::ArgumentTypeMismatch {
                                        index: i,
                                        expected: inst_check_type,
                                        actual: contextual_arg_type,
                                        fallback_return: TypeId::ERROR,
                                    };
                                }
                            }
                        }
                    }
                }
            } else {
                // Add to direct_param_vars when the type parameter appears
                // as a naked (top-level) parameter type, NOT inside a union/intersection.
                // When T appears in `T | string`, inference candidates come from union
                // decomposition and should merge into a union (tsc's getCommonSupertype).
                // When T appears as a naked `T` in multiple parameters (e.g., `x: T, y: T`),
                // first-wins behavior applies for incompatible candidates.
                // This also applies to rest parameters: `foo<T>(...s: T[])` with
                // heterogeneous args uses first-wins to match tsc behavior.
                if let Some(direct_target) = self.direct_inference_tracking_target(target_type) {
                    placeholder_visited.clear();
                    direct_param_vars.extend(self.collect_direct_placeholder_vars_in_type(
                        direct_target,
                        &var_map,
                        &mut placeholder_visited,
                    ));
                }

                // A naked inference placeholder still carries its declared
                // constraint. Validate nullish arguments against that
                // constraint before inference fallback can turn an invalid
                // `null`/`undefined` candidate into the constraint type itself.
                if contextual_arg_type.is_nullish()
                    && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(target_type)
                    && let Some(check_type_id) = tp.constraint
                {
                    let inst_check_type = instantiate_call_type(
                        self.interner,
                        check_type_id,
                        &substitution,
                        actual_this_type,
                    );
                    placeholder_visited.clear();
                    if !self.type_contains_placeholder(
                        inst_check_type,
                        &var_map,
                        &mut placeholder_visited,
                    ) && !self
                        .checker
                        .is_assignable_to(contextual_arg_type, inst_check_type)
                        && !self.is_function_union_compat(contextual_arg_type, inst_check_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: inst_check_type,
                            actual: contextual_arg_type,
                            fallback_return: TypeId::ERROR,
                        };
                    }
                }
            }

            // When the target is a bare type parameter placeholder whose constraint
            // doesn't imply literal types, widen the argument's object literal properties.
            // This matches tsc's behavior: `{ c: false }` passed to parameter `T` becomes
            // `{ c: boolean }` for inference, preventing false TS2322/TS2345 errors.
            let target_has_contextual_seed =
                var_map.get(&contextual_target_type).is_some_and(|&var| {
                    infer_ctx.var_has_candidates(var)
                        || infer_ctx.get_constraints(var).is_some_and(|constraints| {
                            !constraints.lower_bounds.is_empty()
                                || !constraints.upper_bounds.is_empty()
                        })
                });
            let source_for_inference = if widenable_placeholders.contains(&contextual_target_type)
                && !target_has_contextual_seed
            {
                widening::widen_object_literal_properties(self.interner, contextual_arg_type)
            } else {
                contextual_arg_type
            };
            // Contextual typing can leak the caller signature's TypeParams into
            // an argument type before overload-specific placeholders exist. Those
            // leaked params should be rewritten to placeholders for Round 1.
            // Non-contextual sources, including explicit annotations and outer
            // generic values, are real candidates and must stay distinct even
            // when they share a name with the called signature's TypeParams.
            let source_for_inference = if self.is_contextually_sensitive(arg_type) {
                self.substitute_caller_type_params(source_for_inference, &substitution)
            } else {
                source_for_inference
            };
            let source_arg_shape = Self::get_contextual_signature_cached(self.interner, arg_type);
            let original_arg_is_generic_function_like = source_arg_shape
                .as_ref()
                .is_some_and(|shape| !shape.type_params.is_empty());
            let source_for_inference = self.instantiate_generic_function_argument_against_target(
                source_for_inference,
                contextual_target_type,
            );
            let arg_inference_priority = if original_arg_is_generic_function_like
                && self.type_evaluates_to_function(contextual_target_type)
            {
                crate::types::InferencePriority::ReturnType
            } else {
                crate::types::InferencePriority::NakedTypeVariable
            };
            if original_arg_is_generic_function_like
                && self.function_like_placeholder_appears_in_parameter_position(
                    contextual_target_type,
                    &var_map,
                    &mut placeholder_visited,
                )
            {
                let target_vars = self.collect_placeholder_vars_in_type(
                    contextual_target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                );
                let target_var_already_has_direct_candidate = target_vars.iter().any(|var| {
                    infer_ctx.get_constraints(*var).is_some_and(|constraints| {
                        constraints
                            .lower_bounds
                            .iter()
                            .any(|bound| !bound.is_any_unknown_or_error())
                    })
                });
                // The candidate check above is order-dependent: arguments are
                // visited left-to-right, so a pinning argument that appears
                // *after* this generic-function argument has not yet seeded its
                // type variable. Mirror tsc's `SkipGenericFunctions` by also
                // deferring when a sibling concrete argument structurally pins
                // one of the callback's parameter-position type parameters; the
                // round-2 pass then re-infers this argument in the fixed
                // context.
                let sibling_concrete_arg_pins_callback_param =
                    !target_var_already_has_direct_candidate && {
                        let param_pos_vars = self.collect_callback_parameter_placeholder_vars(
                            contextual_target_type,
                            &var_map,
                            &mut placeholder_probe_map,
                            &mut placeholder_visited,
                        );
                        self.callback_parameter_var_pinned_by_sibling_arg(
                            &instantiated_params,
                            arg_types,
                            i,
                            &param_pos_vars,
                            &var_map,
                        )
                    };
                if target_var_already_has_direct_candidate
                    || sibling_concrete_arg_pins_callback_param
                {
                    deferred_generic_function_arg_indices.insert(i);
                    saw_deferred_arg = true;
                    continue;
                }
            }
            if original_arg_is_generic_function_like
                && let Some(expected) = self.conflicting_contextual_signature_instantiation_type(
                    arg_type,
                    contextual_target_type,
                )
            {
                return CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected,
                    actual: arg_type,
                    fallback_return: TypeId::ERROR,
                };
            }
            // For repeated naked type-parameter parameters, tsc keeps the first
            // primitive-family candidate and reports the later conflicting direct
            // argument. A context-sensitive callback in a later parameter can otherwise
            // add enough inference evidence to merge `""` and `3` into a union,
            // incorrectly accepting `g<T>(a: T, b: T, c: (t: T) => T)`.
            //
            // Important exception: when the later argument's type is *nullable* (a
            // union containing `null` or `undefined`), tsc still seeds inference from
            // the non-nullable members. Skipping the whole argument here would drop
            // those candidates and produce an over-narrow `T` (e.g.
            // `equal<T>(a: T, b: T)` called with `("a", "b" | undefined)` would lose
            // the `"b"` candidate and resolve `T = "a" | undefined`, then reject the
            // second argument as `never` — see
            // `compiler/inferenceOfNullableObjectTypesWithCommonBase.ts`).
            // tsc's `getCommonSupertype` strips nullable before tournament reduction
            // and adds it back afterwards, so a nullable literal-union later argument
            // doesn't trigger first-wins skipping there.
            let arg_is_nullable_union = if let Some(TypeData::Union(list_id)) =
                self.interner.lookup(source_for_inference)
            {
                self.interner
                    .type_list(list_id)
                    .iter()
                    .any(|m| m.is_nullable())
            } else {
                false
            };
            if !arg_is_nullable_union
                && let Some(&var) = var_map.get(&contextual_target_type)
                && !is_rest_param_arg
                && direct_param_vars.contains(&var)
                && !matches!(
                    source_for_inference,
                    TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                )
                && let Some(&first_candidate) = first_direct_primitive_candidate.get(&var)
                && let Some(first_base) = self.primitive_base_of(first_candidate)
            {
                let current_base = self.primitive_base_of(source_for_inference);
                if current_base != Some(first_base)
                    && !self
                        .checker
                        .is_assignable_to(source_for_inference, first_candidate)
                {
                    first_direct_primitive_mismatch.get_or_insert((
                        i,
                        first_candidate,
                        source_for_inference,
                    ));
                    continue;
                }
            } else if let Some(&var) = var_map.get(&contextual_target_type)
                && !is_rest_param_arg
                && direct_param_vars.contains(&var)
                && !matches!(
                    source_for_inference,
                    TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                )
                && self.primitive_base_of(source_for_inference).is_some()
            {
                first_direct_primitive_candidate.insert(var, source_for_inference);
            }

            self.constrain_types_for_arg_source(
                i,
                &mut infer_ctx,
                &var_map,
                source_for_inference,
                contextual_target_type,
                arg_inference_priority,
            );

            let source_is_function = self.type_evaluates_to_function(source_for_inference);
            let target_is_function = self.type_evaluates_to_function(contextual_target_type);
            // Skip constrain_return_context_structure when the target contains inference
            // placeholders. The solver's evaluate_type() cannot fully resolve Application
            // types that contain placeholders (it lacks the checker's TypeEnvironment
            // resolver), so function-signature matching on partially evaluated types can
            // introduce spurious upper bounds from unsubstituted TypeParameters in the
            // interface body. The main constrain_types call above already handles
            // Application argument matching correctly via same-base unification.
            placeholder_visited.clear();
            let target_has_placeholders = self.type_contains_placeholder(
                contextual_target_type,
                &var_map,
                &mut placeholder_visited,
            );
            if (source_is_function || target_is_function) && !target_has_placeholders {
                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    source_for_inference,
                    contextual_target_type,
                    arg_inference_priority,
                );
            }

            // Preserve raw same-base application inference even when the structural
            // constraint walker evaluates the applications (e.g. Kind<F, ...> into its
            // conditional/object form). Without this, intermediate higher-order values
            // only infer through contextual return types and lose generic arguments.
            //
            // SKIP when either application evaluates to a Function/Callable type.
            // Function types have variance-sensitive parameters, and direct arg matching
            // would add covariant candidates where contravariant ones are needed.
            // The structural constraint walker (Function-Function arm) handles variance
            // correctly via constrain_parameter_types.
            if let (
                Some(TypeData::Application(arg_app_id)),
                Some(TypeData::Application(target_app_id)),
            ) = (
                self.interner.lookup(source_for_inference),
                self.interner.lookup(contextual_target_type),
            ) {
                let arg_app = self.interner.type_application(arg_app_id);
                let target_app = self.interner.type_application(target_app_id);
                if arg_app.base == target_app.base
                    && arg_app.args.len() == target_app.args.len()
                    && self.should_directly_constrain_same_base_application(
                        source_for_inference,
                        contextual_target_type,
                    )
                {
                    for (arg_inner, target_inner) in arg_app.args.iter().zip(target_app.args.iter())
                    {
                        self.constrain_types_for_arg_source(
                            i,
                            &mut infer_ctx,
                            &var_map,
                            *arg_inner,
                            *target_inner,
                            crate::types::InferencePriority::NakedTypeVariable,
                        );
                    }
                }
            }
        }

        // Process rest tuple in Round 1 (it's non-contextual).
        // Skip when the rest param's type variable also appears in other parameter
        // types (e.g., `call<TS>(handler: (...args: TS) => void, ...args: TS)`).
        // In that case the other parameter provides a more authoritative constraint
        // (e.g., from the handler's callback params), and the rest args should be
        // validated against the inferred type, not used to infer it.
        if let Some((_start, target_type, tuple_type, inference_vars)) = rest_tuple_inference {
            let target_var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
                FxHashMap::from_iter([(target_type, crate::inference::infer::InferenceVar(0))]);
            let appears_in_other_params = instantiated_params
                [..instantiated_params.len().saturating_sub(1)]
                .iter()
                .any(|p| {
                    placeholder_visited.clear();
                    self.type_contains_placeholder(
                        p.type_id,
                        &target_var_map,
                        &mut placeholder_visited,
                    )
                });
            let has_covariant_candidates = var_map
                .get(&target_type)
                .copied()
                .and_then(|var| infer_ctx.get_constraints(var))
                .is_some_and(|constraints| {
                    constraints.lower_bounds.iter().copied().any(|bound| {
                        is_substantive_inference_candidate(
                            self.interner.as_type_database(),
                            bound,
                            &func.type_params,
                            &var_map,
                        )
                    })
                });
            let should_defer_to_other_param =
                appears_in_other_params && (has_covariant_candidates || saw_deferred_arg);
            if !should_defer_to_other_param {
                // Participation in aggregate tuple inference is not itself
                // provenance: a fixed parameter may already have supplied the
                // winning candidate for the same variable. Only variables
                // whose first evidence comes from this aggregate operation may
                // authorize the provisional direct-callback relation.
                let aggregate_only_vars = inference_vars
                    .into_iter()
                    .filter(|&var| !infer_ctx.var_has_candidates(var))
                    .collect::<Vec<_>>();
                self.mark_spread_rest_literal_mode(
                    &mut infer_ctx,
                    func,
                    target_type,
                    tuple_type,
                    &var_map,
                    &type_param_vars,
                );
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    tuple_type,
                    target_type,
                    crate::types::InferencePriority::NakedTypeVariable,
                );
                aggregate_rest_inference_vars = aggregate_only_vars;
            }
        }

        // When the return type is a bare type parameter (e.g., `function wrap<T>(...): T`),
        // and Round 1 did NOT provide any candidates for that variable, AND no deferred
        // argument can provide candidates in Round 2, add the contextual type as a
        // ReturnType candidate. This enables fix_current_variables to fix T to the
        // contextual type, so Round 2 can use it for lambda parameter types.
        //
        // We defer this to AFTER Round 1 to avoid polluting inference when:
        // - A concrete argument already provides a better NakedTypeVariable candidate
        // - A deferred argument references the same type variable (Round 2 will infer
        //   it from that argument, e.g., `o4?.(incr)` where incr provides T = number)
        if let Some((var, ctx_type)) = return_type_bare_var
            && !infer_ctx.var_has_candidates(var)
            && !deferred_arg_covers_return_var
        {
            infer_ctx.add_candidate(var, ctx_type, crate::types::InferencePriority::ReturnType);
        }

        // === Fixing: Resolve variables with enough information ===
        // This "fixes" type variables that have candidates from Round 1,
        // preventing Round 2 from overriding them with lower-priority constraints.
        // Pass the full checker for co/contra resolution so Lazy types can be
        // compared through their extends chains.
        if infer_ctx
            .fix_current_variables_with(Some(|source, target| {
                self.checker.is_assignable_to(source, target)
            }))
            .is_err()
        {
            // Fixing failed - this might indicate a constraint conflict
            // Continue with partial fixing, final resolution will detect errors
        }
        // Build a substitution from fixed variables (Round 1 results).
        // This maps placeholder names to their resolved types, but ONLY for
        // variables that were actually fixed. Unfixed placeholders remain
        // intact so Round 2 can still infer them.
        let mut fixed_subst = TypeSubstitution::new();
        fixed_subst.protect_type_parameters(&func.type_params);
        // #17282: snapshot the raw Round-1 fixes (before any contextual-return
        // override below) so finalization can keep a callback *return*-position
        // variable from being widened by Round-2 callback-body inference — tsc's
        // immutable `InferenceInfo.isFixed`. Captured unconditionally: a generic
        // call is re-resolved several times, and the pass whose result feeds the
        // diagnostic can see the callback arguments already checked (so a
        // `has_context_sensitive_args`/`saw_deferred_arg` gate is false there
        // while the restore is still required). `FxHashMap::default` does not
        // allocate until the first insert and the finalize reads short-circuit on
        // an empty callback-return set, so the no-callback cost stays negligible.
        let mut round1_fixed: FxHashMap<InferenceVar, TypeId> = FxHashMap::default();
        for (i, (tp, &var)) in func
            .type_params
            .iter()
            .zip(type_param_vars.iter())
            .enumerate()
        {
            let resolved = infer_ctx.probe(var);
            if let Some(round1) = resolved {
                // #17282: snapshot the pristine covariant-only fix when an
                // unannotated callback parameter has polluted this pass.
                round1_fixed.insert(var, infer_ctx.round1_fix_snapshot(var, round1));
            }
            let contextual = structural_return_subst.get(tp.name);
            let resolved = match (resolved, contextual) {
                (Some(inferred), Some(contextual))
                    if !direct_param_vars.contains(&var)
                        && self.should_use_contextual_return_substitution(
                            inferred, contextual, &var_map,
                        ) =>
                {
                    Some(contextual)
                }
                (None, Some(contextual)) if !direct_param_vars.contains(&var) => Some(contextual),
                (Some(inferred), _) => Some(inferred),
                (None, _) => None,
            };

            if let Some(resolved) = resolved {
                // This var was fixed in Round 1 or by return context — map its
                // placeholder name to the resolved type.
                let placeholder_atom = type_param_placeholder_atoms[i];
                fixed_subst.insert(placeholder_atom, resolved);
                // Also map the original type param name, in case target_type references it
                fixed_subst.insert(tp.name, resolved);
            }
        }

        // Re-seed inference from `this` after Round 1 fixing.
        // When the `this` type contains variadic tuple patterns like `[...T, ...U]`,
        // the initial seeding (before Round 1) cannot split the source tuple between
        // multiple rest type variables. After Round 1 fixes some variables (e.g. T
        // from argument types), we re-instantiate the expected `this` type with the
        // fixed substitution and re-run constraint collection. This allows the
        // remaining variables (e.g. U) to be inferred from the leftover elements.
        if let Some(expected_this) = func.this_type {
            let has_unfixed = type_param_vars
                .iter()
                .any(|&var| infer_ctx.probe(var).is_none());
            if has_unfixed && !fixed_subst.is_empty() {
                let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
                // Re-instantiate with the fixed_subst so resolved type params
                // are replaced with their inferred types.
                let expected_this_reinst = instantiate_type(
                    self.interner,
                    instantiate_call_type(
                        self.interner,
                        expected_this,
                        &substitution,
                        actual_this_type,
                    ),
                    &fixed_subst,
                );
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    actual_this,
                    expected_this_reinst,
                    crate::types::InferencePriority::NakedTypeVariable,
                );
            }
        }

        // === Round 2: Process contextual arguments ===
        // These are arguments like lambdas that need contextual typing.
        // Now that non-contextual arguments have been processed, we can provide
        // proper contextual types to lambdas based on fixed type variables.
        if saw_deferred_arg {
            let round2_params = if fixed_subst.is_empty() {
                None
            } else {
                Some(
                    instantiated_params
                        .iter()
                        .map(|param| ParamInfo {
                            name: param.name,
                            type_id: instantiate_type(self.interner, param.type_id, &fixed_subst),
                            optional: param.optional,
                            rest: param.rest,
                        })
                        .collect::<Vec<_>>(),
                )
            };
            for (i, &arg_type) in arg_types.iter().enumerate() {
                // #17282: same mask exposure as Round 1.
                self.current_arg_callback_param_unannotated = self
                    .arg_callback_param_unannotated
                    .get(i)
                    .filter(|m| !m.is_empty())
                    .cloned();
                if rest_tuple_start.is_some_and(|start| i >= start) {
                    continue;
                }
                let Some(target_type) =
                    self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
                else {
                    break;
                };
                if self.arg_targets_aggregate_rest_param(&instantiated_params, i, arg_type) {
                    continue;
                }

                let is_deferred_generic_function_arg =
                    deferred_generic_function_arg_indices.contains(&i);

                let conflict_target = if fixed_subst.is_empty() {
                    target_type
                } else {
                    instantiate_type(self.interner, target_type, &fixed_subst)
                };
                if let Some(expected) = self
                    .conflicting_contextual_signature_instantiation_type(arg_type, conflict_target)
                {
                    return CallResult::ArgumentTypeMismatch {
                        index: i,
                        expected,
                        actual: arg_type,
                        fallback_return: TypeId::ERROR,
                    };
                }

                // Only process contextually sensitive arguments in Round 2, plus
                // generic function references that were deferred until direct
                // argument inference fixed their callback parameter context.
                if !self.is_contextually_sensitive(arg_type) && !is_deferred_generic_function_arg {
                    continue;
                }

                // Check if original target_type contains placeholders BEFORE re-instantiation.
                placeholder_visited.clear();
                let original_has_placeholders =
                    self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited);
                let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                    && i >= instantiated_params.len().saturating_sub(1);
                let round2_target_type =
                    if is_deferred_generic_function_arg && !fixed_subst.is_empty() {
                        Some(instantiate_type(self.interner, target_type, &fixed_subst))
                    } else {
                        round2_params.as_ref().and_then(|params| {
                            self.param_type_for_arg_index(params, i, arg_types.len())
                        })
                    };

                if original_has_placeholders
                    && let Some(direct_target) = self.direct_inference_tracking_target(target_type)
                {
                    placeholder_visited.clear();
                    direct_param_vars.extend(self.collect_direct_placeholder_vars_in_type(
                        direct_target,
                        &var_map,
                        &mut placeholder_visited,
                    ));
                }

                if !original_has_placeholders {
                    // No placeholders in original target - direct assignability check
                    let r2_arg_type = self.instantiate_generic_function_argument_against_target(
                        arg_type,
                        target_type,
                    );
                    if !self.checker.is_assignable_to(r2_arg_type, target_type)
                        && !self.is_function_union_compat(r2_arg_type, target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: target_type,
                            actual: r2_arg_type,
                            fallback_return: TypeId::ERROR,
                        };
                    }
                } else {
                    let preserve_callback_parameter_placeholders = !is_deferred_generic_function_arg
                        && self.function_like_placeholder_appears_in_parameter_position(
                            target_type,
                            &var_map,
                            &mut placeholder_visited,
                        );

                    // Re-instantiate target_type with fixed Round 1 results.
                    // This replaces resolved placeholders with their inferred types while
                    // preserving unresolved placeholders for further Round 2 inference.
                    let r2_target = if preserve_callback_parameter_placeholders {
                        target_type
                    } else if let Some(candidate) = round2_target_type {
                        candidate
                    } else if !fixed_subst.is_empty() {
                        let candidate = instantiate_type(self.interner, target_type, &fixed_subst);
                        placeholder_visited.clear();
                        if self.type_contains_placeholder(
                            candidate,
                            &var_map,
                            &mut placeholder_visited,
                        ) {
                            // Mixed case: some placeholders resolved, some remaining.
                            // Use re-instantiated target so resolved params provide
                            // concrete contextual types to callbacks.
                            candidate
                        } else if is_deferred_generic_function_arg {
                            candidate
                        } else if is_rest_param_arg {
                            // Rest arguments like `...args: ConstructorParameters<Ctor>`
                            // need the fully materialized tuple/application target in
                            // Round 2 once `Ctor` has been fixed by earlier arguments.
                            // Reverting to the unresolved wrapper here loses both the
                            // extracted element type for contextual typing and the
                            // concrete assignability surface for the argument.
                            candidate
                        } else {
                            // All placeholders resolved — keep original for constraint
                            // collection to preserve inference variable connection.
                            target_type
                        }
                    } else {
                        target_type
                    };

                    // Collect constraints using the (possibly re-instantiated) target
                    let r2_arg_type = self
                        .instantiate_generic_function_argument_against_target(arg_type, r2_target);
                    // When the target is a bare placeholder (the parameter type is
                    // directly the type variable, e.g., `fn: T`), use NakedTypeVariable
                    // priority so argument inference takes precedence over contextual
                    // return type substitution. Without this, Round 2 constraints for
                    // `T` from direct arguments are all marked ReturnType, causing
                    // `can_apply_contextual_return_substitution` to override the correctly
                    // inferred type with the contextual return type.
                    let r2_priority = if var_map.contains_key(&r2_target) {
                        crate::types::InferencePriority::NakedTypeVariable
                    } else {
                        crate::types::InferencePriority::ReturnType
                    };
                    self.constrain_types(
                        &mut infer_ctx,
                        &var_map,
                        r2_arg_type,
                        r2_target,
                        r2_priority,
                    );

                    let source_is_function = self.type_evaluates_to_function(r2_arg_type);
                    let target_is_function = self.type_evaluates_to_function(r2_target);
                    if source_is_function || target_is_function {
                        self.constrain_return_context_structure(
                            &mut infer_ctx,
                            &var_map,
                            r2_arg_type,
                            r2_target,
                            crate::types::InferencePriority::ReturnType,
                        );
                    }

                    // Special case: If target_type is a function with rest param type parameter,
                    // and arg_type is a function, infer the tuple type from function parameters.
                    // Example: test<A>((x: string) => {}) where A extends any[]
                    // Should infer A = [string]
                    if let Some(TypeData::Function(target_fn_id)) =
                        self.interner.lookup(target_type)
                    {
                        let target_fn = self.interner.function_shape(target_fn_id);
                        if let Some(t_last) = target_fn.params.last()
                            && t_last.rest
                            && var_map.contains_key(&t_last.type_id)
                            && let Some(TypeData::Function(source_fn_id)) =
                                self.interner.lookup(arg_type)
                        {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            // Create tuple from source function's parameters
                            use crate::type_queries::unpack_tuple_rest_parameter;
                            let params_unpacked: Vec<ParamInfo> = source_fn
                                .params
                                .iter()
                                .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                                .collect();

                            let tuple_elements: Vec<TupleElement> = params_unpacked
                                .iter()
                                .map(|p| TupleElement {
                                    type_id: p.type_id,
                                    name: p.name,
                                    optional: p.optional,
                                    rest: p.rest,
                                })
                                .collect();
                            let param_tuple = self.interner.tuple(tuple_elements);

                            // Infer: A = [string, number]
                            if let Some(&var) = var_map.get(&t_last.type_id) {
                                let target_var_map: FxHashMap<
                                    TypeId,
                                    crate::inference::infer::InferenceVar,
                                > = FxHashMap::from_iter([(t_last.type_id, var)]);
                                let appears_in_other_params = target_fn.params
                                    [..target_fn.params.len().saturating_sub(1)]
                                    .iter()
                                    .any(|param| {
                                        placeholder_visited.clear();
                                        self.type_contains_placeholder(
                                            param.type_id,
                                            &target_var_map,
                                            &mut placeholder_visited,
                                        )
                                    });
                                if appears_in_other_params {
                                    continue;
                                }
                                infer_ctx.add_candidate(
                                    var,
                                    param_tuple,
                                    crate::types::InferencePriority::NakedTypeVariable,
                                );
                            }
                        }
                    }
                }
            }
        }

        self.finish_generic_call_resolution(FinishGenericCallResolutionArgs {
            func,
            arg_types,
            actual_this_type,
            infer_ctx,
            substitution: &substitution,
            type_param_vars: &type_param_vars,
            type_param_placeholder_atoms: &type_param_placeholder_atoms,
            var_map: &var_map,
            direct_param_vars: &direct_param_vars,
            callback_placeholder_subst: &callback_placeholder_subst,
            round1_fixed: &round1_fixed,
            noinfer_param_vars: &noinfer_param_vars,
            rest_tuple_target_type,
            aggregate_rest_inference_vars: &aggregate_rest_inference_vars,
            structural_return_subst: &structural_return_subst,
            first_direct_primitive_mismatch,
            saw_deferred_arg,
        })
    }
}
