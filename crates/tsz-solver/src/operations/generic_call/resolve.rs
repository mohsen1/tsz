//! Core generic call resolution (`resolve_generic_call_inner`).

use crate::inference::infer::{InferenceContext, InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
    TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tracing::{debug, trace};

// Reusable scratch `FxHashSet<TypeId>` for `type_contains_placeholder` calls
// in this module. Mirrors the pool pattern from #4722 / #4790 / #4801 /
// #4805 / #4807 / #4810 / #4816.
thread_local! {
    static RESOLVE_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> =
        const { RefCell::new(None) };
}

#[inline]
fn with_resolve_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = RESOLVE_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    RESOLVE_VISITED_POOL.with(|p| {
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

fn is_bare_foreign_type_param(
    interner: &dyn crate::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match interner.lookup(ty) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            !local_type_params.contains(&info.name) && !local_placeholders.contains(&info.name)
        }
        _ => false,
    }
}

fn is_substantive_inference_candidate(
    interner: &dyn crate::TypeDatabase,
    ty: TypeId,
    local_type_params: &FxHashSet<tsz_common::Atom>,
    local_placeholders: &[tsz_common::Atom],
) -> bool {
    !ty.is_any_unknown_or_error()
        && !is_bare_foreign_type_param(interner, ty, local_type_params, local_placeholders)
        && !crate::visitor::contains_type_parameters(interner, ty)
        && !crate::type_queries::contains_infer_types_db(interner, ty)
}

use super::{
    constraint_contains_primitive_constrained_type_param,
    constraint_is_primitive_type_with_resolver, instantiate_call_type, type_implies_literals_deep,
    type_references_placeholder, unique_placeholder_name,
};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn duplicate_single_arg_application_value_shape(&self, arg_type: TypeId) -> Option<TypeId> {
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.interner.lookup(arg_type)
        else {
            return None;
        };
        let shape = self.interner.object_shape(shape_id);
        if shape.properties.len() < 2 {
            return None;
        }

        let mut keys_by_prop = Vec::with_capacity(shape.properties.len());
        let mut counts: FxHashMap<(TypeId, TypeId), usize> = FxHashMap::default();
        for prop in shape.properties.iter() {
            let Some(alias) = self.interner.get_display_alias(prop.type_id) else {
                keys_by_prop.push(None);
                continue;
            };
            let Some(TypeData::Application(app_id)) = self.interner.lookup(alias) else {
                keys_by_prop.push(None);
                continue;
            };
            let app = self.interner.type_application(app_id);
            let Some(&arg) = app.args.first() else {
                keys_by_prop.push(None);
                continue;
            };
            if app.args.len() != 1
                || crate::visitor::literal_string(self.interner.as_type_database(), arg).is_none()
            {
                keys_by_prop.push(None);
                continue;
            }
            let key = (app.base, arg);
            *counts.entry(key).or_default() += 1;
            keys_by_prop.push(Some(key));
        }

        if !counts.values().any(|&count| count > 1) {
            return None;
        }

        let properties = shape
            .properties
            .iter()
            .zip(keys_by_prop)
            .map(|(prop, key)| {
                let is_duplicate =
                    key.is_some_and(|key| counts.get(&key).copied().unwrap_or(0) > 1);
                let type_id = if is_duplicate {
                    TypeId::NEVER
                } else {
                    TypeId::ANY
                };
                PropertyInfo {
                    name: prop.name,
                    type_id,
                    write_type: type_id,
                    optional: prop.optional,
                    readonly: prop.readonly,
                    is_method: prop.is_method,
                    is_class_prototype: prop.is_class_prototype,
                    visibility: prop.visibility,
                    parent_id: prop.parent_id,
                    declaration_order: prop.declaration_order,
                    is_string_named: prop.is_string_named,
                    is_symbol_named: prop.is_symbol_named,
                    single_quoted_name: prop.single_quoted_name,
                }
            })
            .collect();

        Some(self.interner.object(properties))
    }

    fn object_constraint_properties_are_any(&self, constraint: TypeId) -> bool {
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
        match self.interner.lookup(constraint) {
            Some(
                TypeData::Application(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::Mapped(_)
                | TypeData::StringIntrinsic { .. },
            ) => true,
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.raw_instantiated_constraint_may_satisfy(member)),
            Some(
                TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner),
            ) => self.raw_instantiated_constraint_may_satisfy(inner),
            _ => false,
        }
    }

    fn satisfies_raw_instantiated_constraint(
        &mut self,
        source: TypeId,
        constraint: TypeId,
    ) -> bool {
        if !self.raw_instantiated_constraint_may_satisfy(constraint) {
            return false;
        }
        if self.checker.is_assignable_to(source, constraint) {
            return true;
        }
        self.checker
            .expand_type_alias_application(constraint)
            .is_some_and(|expanded| self.checker.is_assignable_to(source, expanded))
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

    fn constrain_types_for_arg_source(
        &mut self,
        arg_index: usize,
        infer_ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
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

    fn type_param_name_if_generic_rest_tuple_param(
        &self,
        func: &FunctionShape,
        type_id: TypeId,
    ) -> Option<tsz_common::Atom> {
        let type_id = self.unwrap_readonly(type_id);
        let Some(TypeData::TypeParameter(info)) = self.interner.lookup(type_id) else {
            return None;
        };

        func.type_params
            .iter()
            .any(|type_param| type_param.name == info.name)
            .then_some(info.name)
    }

    fn generic_rest_tuple_callback_arity_mismatch(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        let rest_param = func.params.last().filter(|param| param.rest)?;
        let rest_type_param =
            self.type_param_name_if_generic_rest_tuple_param(func, rest_param.type_id)?;
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
            if self.type_param_name_if_generic_rest_tuple_param(func, target_rest.type_id)
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
            let (callback_min, callback_max) = self.arg_count_bounds(&source_shape.params);

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
        // Check argument count BEFORE type inference
        // This prevents false positive TS2554 errors for generic functions with optional/rest params
        let (min_args, max_args) = self.arg_count_bounds(&func.params);

        if arg_types.len() < min_args {
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

        let mut infer_ctx = InferenceContext::with_resolver(
            self.interner.as_type_database(),
            self.interner.as_type_resolver(),
        );
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());
        // Store the placeholder atom for each type param var so we can look them
        // up later (e.g., to build fixed_subst after Round 1).  Indexed in
        // parallel with type_param_vars.
        let mut type_param_placeholder_atoms: Vec<tsz_common::Atom> =
            Vec::with_capacity(func.type_params.len());
        let local_type_param_names: FxHashSet<tsz_common::Atom> =
            func.type_params.iter().map(|tp| tp.name).collect();

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

            // Create a unique placeholder type for this inference variable
            // We use a TypeParameter with a special name to track it during constraint collection.
            // Names are globally unique (via PLACEHOLDER_COUNTER) to prevent
            // collisions when nested generic calls create overlapping placeholder sets.
            unique_placeholder_name(&mut placeholder_buf);
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
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
                let is_referenced_in_other_constraints = func.type_params.iter().any(|other_tp| {
                    if other_tp.name == tp.name {
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
        let mut round1_direct_seed_vars = FxHashSet::default();

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
                round1_direct_seed_vars.extend(self.collect_placeholder_vars_in_type(
                    target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                ));
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
                if return_is_union_with_placeholder {
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

        let structural_return_subst =
            self.compute_return_context_substitution(func, self.contextual_type);
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
                            && !shape.type_params.iter().any(|tp| {
                                matches!(
                                    self.interner.lookup(shape.return_type),
                                    Some(TypeData::TypeParameter(info)) if info.name == tp.name
                                )
                            })
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
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);
        let rest_tuple_target_type = rest_tuple_inference
            .as_ref()
            .map(|(_, target_type, _)| *target_type);
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

            let target_type_param_name = var_map.get(&target_type).and_then(|&var| {
                func.type_params
                    .iter()
                    .zip(type_param_vars.iter())
                    .find_map(|(tp, candidate_var)| (*candidate_var == var).then_some(tp.name))
            });

            // Defer a bare-type-parameter argument when a later context-sensitive
            // generic function-like argument depends on the same type parameter.
            // Non-context-sensitive arguments (primitives, variable references) must
            // NOT be deferred: Round 2 only processes context-sensitive arguments,
            // so deferring a non-context-sensitive arg loses its constraint entirely,
            // causing the type parameter to resolve to `unknown`.
            if let Some(type_param_name) = target_type_param_name
                && self.is_contextually_sensitive(arg_type)
                && self.later_generic_function_like_arg_depends_on_type_param(
                    func,
                    arg_types,
                    i,
                    type_param_name,
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
                    // No placeholder in target_type - check assignability directly
                    if !self
                        .checker
                        .is_assignable_to(contextual_arg_type, contextual_target_type)
                        && !self
                            .is_function_union_compat(contextual_arg_type, contextual_target_type)
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
                if target_var_already_has_direct_candidate {
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

            // arg_type <: target_type
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
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
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
                            &local_type_param_names,
                            &type_param_placeholder_atoms,
                        )
                    })
                });
            let should_defer_to_other_param =
                appears_in_other_params && (has_covariant_candidates || saw_deferred_arg);
            if !should_defer_to_other_param {
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    tuple_type,
                    target_type,
                    crate::types::InferencePriority::NakedTypeVariable,
                );
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
        for (i, (tp, &var)) in func
            .type_params
            .iter()
            .zip(type_param_vars.iter())
            .enumerate()
        {
            let resolved = infer_ctx.probe(var);
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

        // 4. Resolve inference variables
        // CRITICAL: Strengthen inter-parameter constraints before resolution
        // This ensures SCC-based cycle unification happens (commit c3ede45a9)
        if infer_ctx.strengthen_constraints().is_err() {
            // Cycle unification failed - this indicates a circularity that cannot be resolved
            // Fall back to resolving without unification (may result in less precise types)
        }

        // 4.5. Resolve source inference variables (from generic function arguments)
        // and substitute them into outer variables' candidates.
        //
        // When a generic function like `list<T>` is passed as an argument, the constraint
        // collector creates fresh inference vars (`__infer_src_*`) for its type params.
        // These may leak into outer variables' candidates as raw TypeParam placeholders.
        // We resolve them here and substitute concrete types back, so the outer resolution
        // sees real types (e.g., `T[]`) instead of opaque placeholders (e.g., `__infer_src_3`).
        //
        // Multi-pass: After substituting resolved source vars into outer candidates,
        // resolve outer vars, then re-derive remaining source vars from outer results.
        {
            let outer_var_set: FxHashSet<InferenceVar> = type_param_vars.iter().copied().collect();
            let mut source_subst = TypeSubstitution::new();
            let type_params_snapshot: Vec<_> = infer_ctx.type_params.clone();
            // Pass 1: Resolve source vars with direct candidates (not unknown)
            for (name, var, _) in &type_params_snapshot {
                if !outer_var_set.contains(var)
                    && let Ok(resolved) = infer_ctx.resolve_with_constraints(*var)
                    && resolved != TypeId::UNKNOWN
                {
                    source_subst.insert(*name, resolved);
                }
            }
            if !source_subst.is_empty() {
                infer_ctx.substitute_source_vars_in_targets(
                    &type_param_vars,
                    &source_subst,
                    self.interner,
                );
            }
        }

        let mut final_subst = TypeSubstitution::new();
        let mut infer_subst_cache: Option<TypeSubstitution> = None;
        // Track type parameters that fell back to their defaults because inference
        // produced no candidates. For these, we should NOT check argument assignability
        // against the default - the default is a fallback, not a constraint.
        let mut default_fallback_tp_names: FxHashSet<tsz_common::Atom> = FxHashSet::default();
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            let constraints = infer_ctx.get_constraints(var);
            // Check both ConstraintSet (covariant candidates + upper bounds) and
            // usable contra_candidates. Contra-candidates are NOT in
            // ConstraintSet.lower_bounds to avoid polluting the resolved_direct path,
            // but they still represent valid inference that should trigger resolution.
            // Ignore only synthetic placeholder type parameters; real outer type
            // parameters like `T` must still count as usable evidence.
            let has_constraints = matches!(&constraints, Some(c) if !c.is_empty())
                || infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database());
            let has_only_declared_upper_bounds = tp.default.is_some()
                && !infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database())
                && constraints
                    .as_ref()
                    .is_some_and(|c| c.lower_bounds.is_empty() && !c.upper_bounds.is_empty());
            let lower_bounds = constraints
                .as_ref()
                .map(|c| c.lower_bounds.clone())
                .unwrap_or_default();
            trace!(
                type_param_name = ?self.interner.resolve_atom(tp.name),
                var = ?var,
                has_constraints = has_constraints,
                constraints = ?constraints,
                has_default = tp.default.is_some(),
                has_constraint = tp.constraint.is_some(),
                constraint = ?tp.constraint,
                "Resolving type parameter"
            );
            let ty = if has_constraints && !has_only_declared_upper_bounds {
                let mut resolved_direct = None;
                let contra_only = infer_ctx.has_only_contra_candidates(var);
                let has_usable_contra_candidates =
                    infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database());

                if direct_param_vars.contains(&var)
                    && let Some(constraint_ty) = tp.constraint
                    && let Some(constraints) = constraints.as_ref()
                    && constraints.lower_bounds.contains(&constraint_ty)
                {
                    let mut non_constraint_bounds = Vec::new();
                    for bound in &constraints.lower_bounds {
                        if *bound != constraint_ty && !non_constraint_bounds.contains(bound) {
                            non_constraint_bounds.push(*bound);
                        }
                    }

                    if !non_constraint_bounds.is_empty() {
                        // When all non-constraint bounds are subtypes of the constraint,
                        // the constraint is the correct inference result (it's the best
                        // common supertype). Stripping it would incorrectly narrow T to
                        // a subtype. Example: foo<T extends C>(t: X<T>, t2: X<T>) called
                        // with X<C> and X<D> where D extends C — T should be C, not D.
                        let all_subtypes_of_constraint = non_constraint_bounds
                            .iter()
                            .all(|&bound| self.checker.is_assignable_to(bound, constraint_ty));
                        if !all_subtypes_of_constraint {
                            let candidate = self.resolve_direct_parameter_inference_type(
                                &non_constraint_bounds,
                                infer_ctx.best_common_type(&non_constraint_bounds),
                                has_usable_contra_candidates,
                            );
                            let upper_bounds_ok = constraints.upper_bounds.iter().all(|upper| {
                                !matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                                    && infer_ctx.is_subtype(candidate, *upper)
                                    || matches!(
                                        upper,
                                        &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR
                                    )
                            });

                            if upper_bounds_ok {
                                resolved_direct = Some(candidate);
                            }
                        }
                    }
                }

                let has_index_signature_candidates = infer_ctx.has_index_signature_candidates(var);
                let ty = if let Some(resolved) = resolved_direct {
                    let root = infer_ctx.table.find(var);
                    let mut info = infer_ctx.table.probe_value(root);
                    info.resolved = Some(resolved);
                    infer_ctx.table.union_value(root, info);
                    resolved
                } else {
                    match infer_ctx.resolve_with_constraints_by(var, |source, target| {
                        self.checker.is_assignable_to_strict(source, target)
                    }) {
                        Ok(ty) => {
                            let all_return_type = infer_ctx.all_candidates_are_return_type(var);
                            trace!(
                                var = ?var,
                                lower_bounds = ?lower_bounds,
                                direct_param = direct_param_vars.contains(&var),
                                all_return_type = all_return_type,
                                pre_adjusted = ?ty,
                                "Adjusting resolved inference type"
                            );
                            let mut ty = if all_return_type {
                                self.resolve_return_position_inference_type(&lower_bounds, ty)
                            } else if direct_param_vars.contains(&var)
                                && !has_index_signature_candidates
                            {
                                self.resolve_direct_parameter_inference_type(
                                    &lower_bounds,
                                    ty,
                                    has_usable_contra_candidates,
                                )
                            } else {
                                ty
                            };
                            if direct_param_vars.contains(&var) && has_usable_contra_candidates {
                                let contra_types = infer_ctx.get_contra_candidate_types(var);
                                let concrete_contra: Vec<_> = contra_types
                                    .into_iter()
                                    .filter(|contra| {
                                        !crate::type_queries::data::is_bare_current_infer_placeholder_db(
                                            self.interner.as_type_database(),
                                            *contra,
                                        )
                                    })
                                    .collect();
                                if concrete_contra.len() == 1 {
                                    let contra = concrete_contra[0];
                                    if self
                                        .should_prefer_single_contra_candidate_for_direct_inference(
                                            &lower_bounds,
                                            ty,
                                            contra,
                                        )
                                    {
                                        ty = self
                                            .select_single_contra_candidate_direct_inference_type(
                                                &lower_bounds,
                                                contra,
                                            );
                                        let root = infer_ctx.table.find(var);
                                        let mut info = infer_ctx.table.probe_value(root);
                                        info.resolved = Some(ty);
                                        infer_ctx.table.union_value(root, info);
                                    }

                                    let mut needs_broader_due_dependent_constraint = false;
                                    if lower_bounds.len() == 1
                                        && self.checker.is_assignable_to(ty, contra)
                                        && !self.checker.is_assignable_to(contra, ty)
                                    {
                                        for (other_tp, &other_var) in
                                            func.type_params.iter().zip(type_param_vars.iter())
                                        {
                                            if other_tp.name == tp.name {
                                                continue;
                                            }
                                            let Some(other_constraint) = other_tp.constraint else {
                                                continue;
                                            };
                                            let direct_constraint_on_current =
                                                crate::type_param_info(
                                                    self.interner.as_type_database(),
                                                    other_constraint,
                                                )
                                                .is_some_and(|info| info.name == tp.name);
                                            if !direct_constraint_on_current
                                                && !crate::visitors::visitor_predicates::contains_type_parameter_named(
                                                    self.interner,
                                                    other_constraint,
                                                    tp.name,
                                                )
                                            {
                                                continue;
                                            }
                                            let Some(other_constraints) =
                                                infer_ctx.get_constraints(other_var)
                                            else {
                                                continue;
                                            };
                                            for lb in other_constraints.lower_bounds.iter().copied()
                                            {
                                                if lb.is_any_unknown_or_error() {
                                                    continue;
                                                }
                                                if !self.checker.is_assignable_to(lb, ty)
                                                    && self.checker.is_assignable_to(lb, contra)
                                                {
                                                    needs_broader_due_dependent_constraint = true;
                                                    break;
                                                }
                                            }
                                            if needs_broader_due_dependent_constraint {
                                                break;
                                            }
                                        }
                                    }

                                    if needs_broader_due_dependent_constraint {
                                        ty = contra;
                                    }
                                }
                            }
                            trace!(
                                resolved_type = ?ty,
                                "Type parameter resolved successfully from constraints"
                            );
                            ty
                        }
                        Err(e) => {
                            trace!(
                                error = ?e,
                                "Constraint resolution failed, using fallback"
                            );

                            // When the bounds violation comes from callback return type
                            // inference (Round 2, ReturnType priority), tsc uses the inferred
                            // type and reports TS2322 on the return expression rather than
                            // falling back to the constraint and reporting TS2345 on the
                            // whole callback argument.
                            let use_inferred = matches!(&e, InferenceError::BoundsViolation { .. })
                                && infer_ctx.all_candidates_are_return_type(var)
                                && saw_deferred_arg;

                            let fallback = if use_inferred {
                                // Use the inferred type (lower bound from BoundsViolation)
                                if let InferenceError::BoundsViolation { lower, .. } = &e {
                                    *lower
                                } else {
                                    // Missing bounds violation during inferred fallback: continue without constraint error
                                    TypeId::ERROR
                                }
                            } else if let Some(upper) =
                                self.single_concrete_upper_bound(&mut infer_ctx, var)
                            {
                                upper
                            } else if let Some(default) = tp.default {
                                instantiate_call_type(
                                    self.interner,
                                    default,
                                    &final_subst,
                                    actual_this_type,
                                )
                            } else if let Some(constraint) = tp.constraint {
                                instantiate_call_type(
                                    self.interner,
                                    constraint,
                                    &final_subst,
                                    actual_this_type,
                                )
                            } else {
                                TypeId::ERROR
                            };
                            let fallback = if direct_param_vars.contains(&var)
                                && !has_index_signature_candidates
                            {
                                self.resolve_direct_parameter_inference_type(
                                    &lower_bounds,
                                    fallback,
                                    has_usable_contra_candidates,
                                )
                            } else {
                                fallback
                            };
                            trace!(
                                fallback_type = ?fallback,
                                use_inferred = use_inferred,
                                "Using fallback type"
                            );
                            fallback
                        }
                    }
                };

                // Generic source contextual instantiation can produce temporary placeholders
                // (e.g. `__infer_src_*`) while collecting constraints for callback arguments.
                // Those placeholders must never leak into final instantiated signatures.
                if saw_deferred_arg {
                    let infer_subst = if let Some(ref cached) = infer_subst_cache {
                        cached
                    } else {
                        let mut subst = infer_ctx.get_current_substitution();
                        self.remove_unresolved_source_placeholders_from_substitution(&mut subst);
                        infer_subst_cache = Some(subst);
                        infer_subst_cache
                            .as_ref()
                            .expect("inference substitution cache just initialized")
                    };
                    self.normalize_inferred_placeholder_type_preserving_source_placeholders(
                        ty,
                        infer_subst,
                    )
                } else {
                    let constraint_preserves_literals = if let Some(constraint) = tp.constraint {
                        let instantiated_constraint = instantiate_call_type(
                            self.interner,
                            constraint,
                            &substitution,
                            actual_this_type,
                        );
                        let resolver = self
                            .checker
                            .type_resolver()
                            .unwrap_or_else(|| self.interner.as_type_resolver());
                        type_implies_literals_deep(self.interner, instantiated_constraint)
                            || constraint_is_primitive_type_with_resolver(
                                self.interner,
                                resolver,
                                instantiated_constraint,
                            )
                            || constraint_contains_primitive_constrained_type_param(
                                self.interner,
                                resolver,
                                instantiated_constraint,
                                0,
                            )
                    } else {
                        false
                    };
                    if !tp.is_const && !contra_only && !constraint_preserves_literals {
                        // Widen fresh inference results from expressions when the type
                        // parameter does NOT have a primitive literal-preserving constraint.
                        // tsc preserves literal types when the constraint is a primitive:
                        //   <T extends string>(a: T) => T  -- T="z" preserved
                        //   <T>(a: T) => T                  -- handled by the trivial fast path
                        if infer_ctx.all_candidates_are_fresh_literals(var) {
                            if noinfer_param_vars.contains(&var) {
                                let mut literal_bounds = lower_bounds
                                    .iter()
                                    .copied()
                                    .filter(|bound| !bound.is_any_unknown_or_error())
                                    .collect::<Vec<_>>();
                                literal_bounds.dedup();
                                if literal_bounds.is_empty() {
                                    ty
                                } else {
                                    let result = crate::utils::union_or_single(
                                        self.interner,
                                        literal_bounds,
                                    );
                                    // tsc's BCT widening: array element inference widens
                                    // fresh literals to their primitive in NoInfer<T>
                                    // positions. Direct scalar arguments are NOT widened
                                    // (from_array_element = false on their candidates).
                                    let db = self.interner.as_type_database();
                                    let should_widen =
                                        (crate::visitor::is_literal_type(db, result)
                                            && infer_ctx.all_candidates_from_array_elements(var))
                                            || crate::visitor::is_union_of_fresh_literals(
                                                db, result,
                                            );
                                    if should_widen {
                                        crate::widen_literal_type(db, result)
                                    } else {
                                        result
                                    }
                                }
                            } else {
                                crate::widen_literal_type(self.interner.as_type_database(), ty)
                            }
                        } else if self.inference_type_contains_fresh_object_or_array(ty)
                            && !infer_ctx.has_type_annotation_candidates(var)
                        {
                            crate::operations::widening::widen_type_for_inference(
                                self.interner.as_type_database(),
                                ty,
                            )
                        } else {
                            ty
                        }
                    } else {
                        ty
                    }
                }
            } else if let Some(default) = tp.default {
                let ty =
                    instantiate_call_type(self.interner, default, &final_subst, actual_this_type);
                trace!(resolved_type = ?ty, "Using default type");
                // Track that this type parameter fell back to its default.
                // We should NOT check argument assignability against the default
                // since it's a fallback when inference fails, not a constraint.
                default_fallback_tp_names.insert(tp.name);
                ty
            } else if let Some(constraint) = tp.constraint {
                let ty = instantiate_call_type(
                    self.interner,
                    constraint,
                    &final_subst,
                    actual_this_type,
                );
                trace!(resolved_type = ?ty, "Using constraint as fallback (no constraints collected)");
                ty
            } else {
                trace!("Using UNKNOWN (unconstrained type parameter)");
                // TypeScript infers 'unknown' for unconstrained type parameters without defaults
                TypeId::UNKNOWN
            };

            let has_rest_tuple_evidence = rest_tuple_target_type
                .and_then(|target_type| var_map.get(&target_type).copied())
                .is_some_and(|rest_var| rest_var == var);
            let ty = if has_rest_tuple_evidence
                && is_bare_foreign_type_param(
                    self.interner.as_type_database(),
                    ty,
                    &local_type_param_names,
                    &type_param_placeholder_atoms,
                ) {
                let concrete_lower_bounds = lower_bounds
                    .iter()
                    .copied()
                    .filter(|&bound| {
                        is_substantive_inference_candidate(
                            self.interner.as_type_database(),
                            bound,
                            &local_type_param_names,
                            &type_param_placeholder_atoms,
                        )
                    })
                    .collect::<Vec<_>>();
                match concrete_lower_bounds.as_slice() {
                    [] => ty,
                    [single] => *single,
                    bounds => infer_ctx.best_common_type(bounds),
                }
            } else {
                ty
            };
            let type_param_name = self.interner.resolve_atom(tp.name);
            let ty = if let Some(contextual_ty) = structural_return_subst.get(tp.name) {
                let contextual_can_replace_foreign_source = is_bare_foreign_type_param(
                    self.interner.as_type_database(),
                    ty,
                    &local_type_param_names,
                    &type_param_placeholder_atoms,
                ) && infer_ctx
                    .all_candidates_are_return_type(var);
                // When a type parameter had NO inference candidates at all
                // (has_constraints=false) and defaulted to `unknown`, AND the type
                // parameter was referenced in a non-deferred argument position
                // (direct_param_vars contains it), the contextual return substitution
                // must NOT override it. The `unknown` result means the argument types
                // genuinely provide no inference information for this type parameter
                // (e.g., `NumberMap<Function>` passed to `StringMap<T>` where number
                // index doesn't satisfy string index). Overriding with the contextual
                // type (e.g., `Function` from `var v1: Function[]`) would mask the
                // type mismatch that should produce TS2403 for redeclarations.
                //
                // However, when the type parameter was NOT in a direct parameter
                // position (only in deferred/context-sensitive args), the `unknown`
                // is a placeholder that SHOULD be replaced by the contextual return
                // type to enable proper contextual typing of callbacks.
                let constructor_context_can_fill_unknown =
                    func.is_constructor && structural_return_subst.get(tp.name).is_some();
                let prefer_contextual_constraint_candidate = if direct_param_vars.contains(&var) {
                    if let Some(constraint) = tp.constraint {
                        let constraint_ty_raw = instantiate_call_type(
                            self.interner,
                            constraint,
                            &final_subst,
                            actual_this_type,
                        );
                        let constraint_ty = self.checker.evaluate_type(constraint_ty_raw);
                        let ty_for_check =
                            crate::relations::freshness::widen_freshness(self.interner, ty);
                        let contextual_for_check = crate::relations::freshness::widen_freshness(
                            self.interner,
                            contextual_ty,
                        );
                        let ty_satisfies_raw = constraint_ty_raw != constraint_ty
                            && self.satisfies_raw_instantiated_constraint(
                                ty_for_check,
                                constraint_ty_raw,
                            );
                        let contextual_satisfies_raw = constraint_ty_raw != constraint_ty
                            && self.satisfies_raw_instantiated_constraint(
                                contextual_for_check,
                                constraint_ty_raw,
                            );
                        let ty_satisfies_constraint = ty_satisfies_raw
                            || self.checker.is_assignable_to(ty_for_check, constraint_ty);
                        let contextual_satisfies_constraint = contextual_satisfies_raw
                            || self
                                .checker
                                .is_assignable_to(contextual_for_check, constraint_ty);
                        !ty_satisfies_constraint && contextual_satisfies_constraint
                    } else {
                        false
                    }
                } else {
                    false
                };
                let keep_direct_param_inference = direct_param_vars.contains(&var)
                    && !contextual_can_replace_foreign_source
                    && !prefer_contextual_constraint_candidate
                    && ((!has_constraints
                        && ty == TypeId::UNKNOWN
                        && !constructor_context_can_fill_unknown)
                        || (ty != TypeId::UNKNOWN && ty != TypeId::ERROR));
                if keep_direct_param_inference {
                    ty
                } else {
                    let can_apply = self.can_apply_contextual_return_substitution(
                        &mut infer_ctx,
                        var,
                        ty,
                        &var_map,
                    );
                    let should_use = contextual_can_replace_foreign_source
                        || self.should_use_contextual_return_substitution(
                            ty,
                            contextual_ty,
                            &var_map,
                        );
                    // When the variable was NOT inferred from a direct parameter match
                    // (i.e., it was inferred structurally from e.g. callback return types),
                    // allow the contextual return substitution to override even when
                    // can_apply would normally block it. This handles cases like:
                    //   let xx: 0 | 1 | 2 = invoke(() => 1);
                    // where T gets NakedTypeVariable candidate `number` from the lambda
                    // return type, but the contextual type `0 | 1 | 2` is strictly narrower
                    // and should take priority. Direct parameter vars (e.g., `foo<T>(x: T)`)
                    // are excluded because their inference is authoritative.
                    let indirect_narrowing_override =
                        !direct_param_vars.contains(&var) && should_use && !can_apply;
                    if (can_apply && should_use) || indirect_narrowing_override {
                        contextual_ty
                    } else {
                        ty
                    }
                }
            } else {
                ty
            };
            trace!(
                type_param_name = %type_param_name.as_str(),
                var = ?var,
                resolved_type = ty.0,
                resolved_type_key = ?self.interner.lookup(ty),
                "Resolved type parameter"
            );
            final_subst.insert(tp.name, ty);
        }

        // Recursively resolve placeholders in final_subst.
        // If an inferred type contains transient placeholders from source functions (e.g. __infer_src_U),
        // we must resolve them using the full inference context substitution.
        // Example: B -> Array(__infer_src_U) where __infer_src_U -> T. We want B -> Array(T).
        {
            let mut full_subst = infer_ctx.get_current_substitution();
            self.remove_unresolved_source_placeholders_from_substitution(&mut full_subst);
            let mut resolved_subst = TypeSubstitution::new();
            for (name, ty) in final_subst.map().iter() {
                let mut placeholder_visited = FxHashSet::default();
                if structural_return_subst.get(*name) == Some(*ty)
                    && !self.type_contains_placeholder(*ty, &var_map, &mut placeholder_visited)
                {
                    resolved_subst.insert(*name, *ty);
                    continue;
                }
                // Iteratively apply substitution to resolve transitive placeholders.
                let mut current = *ty;
                for _ in 0..8 {
                    let next = instantiate_type(self.interner, current, &full_subst);
                    if next == current {
                        break;
                    }
                    current = next;
                }
                resolved_subst.insert(*name, current);
            }
            // Update final_subst with fully resolved types
            for (name, ty) in resolved_subst.map().iter() {
                final_subst.insert(*name, *ty);
            }
        }

        // Constraint checking is deferred until ALL type parameters are resolved.
        // This handles cases like `<T extends U, U>` where T's constraint references
        // U, which may not be in final_subst until later iterations.
        let mut constraint_fallback_tp_names: FxHashSet<tsz_common::Atom> = FxHashSet::default();
        let mut constraint_fallback_display_types: FxHashMap<tsz_common::Atom, TypeId> =
            FxHashMap::default();
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(constraint) = tp.constraint {
                let ty = final_subst.get(tp.name).unwrap_or(TypeId::ERROR);
                if crate::visitors::visitor_predicates::contains_infer_types(
                    self.interner.as_type_database(),
                    constraint,
                ) {
                    final_subst.insert(tp.name, ty);
                    continue;
                }
                let constraint_ty_raw = instantiate_call_type(
                    self.interner,
                    constraint,
                    &final_subst,
                    actual_this_type,
                );
                // Evaluate the instantiated constraint so concrete conditionals like
                // `null extends string ? any : never` resolve to their branch (`never`)
                // instead of remaining as unevaluated Conditional types.
                let constraint_ty = self.checker.evaluate_type(constraint_ty_raw);
                // When the constraint is a deferred `keyof T` where T is a type parameter,
                // skip the constraint validation. TypeScript defers this check to
                // instantiation time because `keyof T` can't be resolved until T is known.
                // Without this, `K extends keyof T` with inferred K = "content" fails
                // even when T extends { content: C }.
                if let Some(keyof_operand) =
                    crate::visitor::keyof_inner_type(self.interner, constraint_ty)
                    && matches!(
                        self.interner.lookup(keyof_operand),
                        Some(crate::TypeData::TypeParameter(_))
                    )
                {
                    final_subst.insert(tp.name, ty);
                    continue;
                }
                // Strip freshness before constraint check: inferred types should not
                // trigger excess property checking against type parameter constraints.
                let ty_for_check = crate::relations::freshness::widen_freshness(self.interner, ty);
                let raw_constraint_satisfied = constraint_ty_raw != constraint_ty
                    && self.satisfies_raw_instantiated_constraint(ty_for_check, constraint_ty_raw);
                if !self.checker.is_assignable_to(ty_for_check, constraint_ty)
                    && !raw_constraint_satisfied
                    && !self.callable_satisfies_top_rest_any_constraint(ty_for_check, constraint_ty)
                {
                    // When the inferred type is a TypeParameter whose own constraint
                    // is structurally equivalent to the target constraint, accept it.
                    // This handles: K extends keyof S passed to K2 extends keyof S
                    // where the two keyof S expressions use different TypeParameter
                    // TypeIds for S (because Store<S> instantiation created a new S).
                    // When the inferred type is a TypeParameter whose constraint
                    // is structurally the same as the target constraint, accept it.
                    // This handles cross-context type parameter identity:
                    // K extends keyof S passed to K2 extends keyof S where
                    // the two S params have different TypeIds but same name.
                    if let Some(crate::TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(ty_for_check)
                        && let Some(tp_constraint) = tp_info.constraint
                    {
                        // Direct TypeId match
                        if tp_constraint == constraint_ty {
                            final_subst.insert(tp.name, ty_for_check);
                            continue;
                        }
                        // Both are keyof <TypeParam> with same-named params
                        if let (Some(c_inner), Some(t_inner)) = (
                            crate::visitor::keyof_inner_type(self.interner, tp_constraint),
                            crate::visitor::keyof_inner_type(self.interner, constraint_ty),
                        ) && let (
                            Some(crate::TypeData::TypeParameter(c_tp)),
                            Some(crate::TypeData::TypeParameter(t_tp)),
                        ) = (self.interner.lookup(c_inner), self.interner.lookup(t_inner))
                            && c_tp.name == t_tp.name
                        {
                            final_subst.insert(tp.name, ty_for_check);
                            continue;
                        }
                        // When the inferred type is a TypeParameter from an outer
                        // scope, its constraint is guaranteed to be at least as
                        // specific as the function's type parameter constraint
                        // (the inference already validated upper bounds during
                        // resolution). Accept the TypeParameter to preserve the
                        // more specific type information instead of collapsing
                        // to the constraint. This handles cases like:
                        //   U extends MessageList<T>, MessageList<T> extends Message
                        //   → U satisfies V extends Message
                        // where structural comparison may fail due to `this` types
                        // or unresolved Application types in the constraint chain.
                        final_subst.insert(tp.name, ty_for_check);
                        continue;
                    }
                    // Lazy(DefId) from contextual return inference may fail structural
                    // constraint checks due to evaluation differences in complex
                    // inheritance chains (e.g., DOM). Keep it for non-direct
                    // inference; direct argument inference still has to report
                    // constraint failures on the argument site.
                    if !direct_param_vars.contains(&var)
                        && matches!(self.interner.lookup(ty), Some(TypeData::Lazy(_)))
                    {
                        final_subst.insert(tp.name, ty);
                        continue;
                    }
                    // Try to recover using un-widened literal candidates when widening
                    // caused the violation (e.g., "b" widened to string violates keyof O).
                    let un_widened = infer_ctx.get_literal_candidates(var);
                    let candidate_type = if !un_widened.is_empty() {
                        Some(if un_widened.len() == 1 {
                            un_widened[0]
                        } else {
                            self.interner.union_from_slice(&un_widened)
                        })
                    } else {
                        None
                    };
                    let recovered = if let Some(candidate_type) = candidate_type {
                        let candidate_satisfies_raw = constraint_ty_raw != constraint_ty
                            && self.satisfies_raw_instantiated_constraint(
                                candidate_type,
                                constraint_ty_raw,
                            );
                        if self.checker.is_assignable_to(candidate_type, constraint_ty)
                            || candidate_satisfies_raw
                        {
                            Some(candidate_type)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(recovered_ty) = recovered {
                        final_subst.insert(tp.name, recovered_ty);
                    } else {
                        if let Some(candidate_type) = candidate_type {
                            let previous = final_subst.get(tp.name);
                            final_subst.insert(tp.name, candidate_type);
                            let display_constraint = instantiate_call_type(
                                self.interner,
                                constraint,
                                &final_subst,
                                actual_this_type,
                            );
                            if let Some(previous) = previous {
                                final_subst.insert(tp.name, previous);
                            } else {
                                final_subst.remove(tp.name);
                            }
                            constraint_fallback_display_types.insert(tp.name, display_constraint);
                        }
                        // Fall back to constraint type so argument checking emits TS2345
                        final_subst.insert(tp.name, constraint_ty);
                        constraint_fallback_tp_names.insert(tp.name);
                    }
                }
            }
        }

        // Circular-inference guard for unconstrained type parameters.
        //
        // When an unconstrained T_inner is inferred as a composite type that
        // structurally CONTAINS a foreign outer-scope placeholder (e.g.
        // `T_outer & object`), the final argument assignability check becomes
        // tautological: `T_outer & object <: T_outer & object` trivially passes,
        // so TS2345 is never emitted even though the expression is unsound.
        //
        // tsc detects this and emits TS2345. We match that behaviour by
        // reverting `final_subst[T_inner.name]` back to the call-local
        // placeholder TypeId whenever all four conditions hold:
        //   1. The type parameter has no constraint (unconstrained).
        //   2. Inference produced usable contra-variance candidates (meaning the
        //      outer call constrained the parameter; prevents false positives for
        //      independent generic calls like `identity(value[key])`).
        //   3. At least one covariant candidate is an IndexAccess type (the
        //      structural marker of `T[K]` being passed to `T`). Pure outer-T
        //      forwarding (`T_outer[]` → `T_inner`) never has an IndexAccess
        //      covariant candidate and must not be reverted.
        //   4. The inferred type structurally contains a foreign TypeParameter
        //      (from an outer scope), making the post-substitution check
        //      tautological.
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            // Condition 1: type parameter is unconstrained.
            if tp.constraint.is_some() {
                continue;
            }
            let Some(inferred_ty) = final_subst.get(tp.name) else {
                continue;
            };
            // Condition 2: had usable contra candidates during inference.
            if !infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database()) {
                continue;
            }
            // Condition 3: at least one covariant candidate is an IndexAccess type.
            if !infer_ctx.has_index_access_covariant_candidate(var) {
                continue;
            }
            // Condition 4: the inferred type structurally contains a foreign TypeParameter.
            if !self.type_contains_any_foreign_type_param(inferred_ty, &var_map) {
                continue;
            }
            // Revert to the call-local placeholder so the argument check is
            // non-tautological and TS2345 can fire.
            if let Some((&pid, _)) = var_map.iter().find(|(_, v)| **v == var) {
                final_subst.insert(tp.name, pid);
            }
        }

        // Check if the rest param's type parameter was explicitly replaced by
        // its constraint during the fallback path above. This only matches when
        // the constraint check FAILED and the code fell through to the fallback
        // (not when the constraint was naturally resolved as the inferred type).
        // Also handles variadic tuple rest params like `readonly [...S, number]`
        // where S is a type parameter from constraint fallback.
        let rest_param_from_constraint_fallback = func.params.last().is_some_and(|p| {
            if !p.rest {
                return false;
            }
            // Direct TypeParameter rest param (e.g., `...args: T`)
            if let Some(crate::TypeData::TypeParameter(tp_info)) = self.interner.lookup(p.type_id)
                && constraint_fallback_tp_names.contains(&tp_info.name)
            {
                return true;
            }
            // Variadic tuple rest param (e.g., `...args: readonly [...S, number]`)
            // where S is a type parameter that fell back to its constraint.
            let unwrapped = self.unwrap_readonly(p.type_id);
            if let Some(crate::TypeData::Tuple(elements)) = self.interner.lookup(unwrapped) {
                let elements = self.interner.tuple_list(elements);
                return elements.iter().any(|elem| {
                    if elem.rest
                        && let Some(crate::TypeData::TypeParameter(tp_info)) =
                            self.interner.lookup(elem.type_id)
                    {
                        return constraint_fallback_tp_names.contains(&tp_info.name);
                    }
                    false
                });
            }
            false
        });

        if let Some(rest_param) = func.params.last().filter(|param| param.rest) {
            let rest_start = func.params.len().saturating_sub(1);
            if arg_types.len() == rest_start {
                let rest_type = instantiate_call_type(
                    self.interner,
                    rest_param.type_id,
                    &final_subst,
                    actual_this_type,
                );
                let rest_type = self.unwrap_readonly(rest_type);
                let evaluated_rest_type = self.evaluate_rest_param_type(rest_type);
                if self.rest_type_needs_aggregate_argument_check(evaluated_rest_type)
                    && let Some(TypeData::Application(app_id)) = self
                        .interner
                        .lookup(self.unwrap_readonly(rest_param.type_id))
                {
                    let app = self.interner.type_application(app_id);
                    for &arg in app.args.iter() {
                        if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(arg)
                            && final_subst.get(info.name) == Some(TypeId::UNKNOWN)
                        {
                            final_subst.insert(info.name, TypeId::NEVER);
                        }
                    }
                }
            }
        }

        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated =
                    instantiate_call_type(self.interner, p.type_id, &final_subst, actual_this_type);
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();
        if !rest_param_from_constraint_fallback {
            let (min_args, max_args) = self.arg_count_bounds(&instantiated_params);
            if arg_types.len() < min_args {
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
        }

        // Validate `this` after final substitution so generic `this` params are fully
        // instantiated (e.g. `this: T` -> `this: Y`).
        if let Some(expected_this) = func.this_type {
            let expected_this =
                instantiate_call_type(self.interner, expected_this, &final_subst, actual_this_type);
            let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
            if !self.checker.is_assignable_to(actual_this, expected_this) {
                return CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this,
                    emit_not_callable: false,
                };
            }
        }

        // Final check: verify arguments against instantiated parameters.
        // When callbacks are contextually typed with the callee's inference placeholders
        // (__infer_0, etc.), those placeholders leak into the arg types. Replace them
        // with the inferred values before the assignability check. Using placeholder
        // names avoids name collisions with same-named type parameters from outer scopes.
        let placeholder_subst = {
            let mut s = TypeSubstitution::new();
            for (i, tp) in func.type_params.iter().enumerate() {
                if let Some(inferred) = final_subst.get(tp.name) {
                    let placeholder_atom = type_param_placeholder_atoms[i];
                    s.insert(placeholder_atom, inferred);
                }
            }
            s
        };
        let mut final_arg_subst = infer_ctx.get_current_substitution();
        self.remove_unresolved_source_placeholders_from_substitution(&mut final_arg_subst);
        for (name, ty) in placeholder_subst.map().iter() {
            final_arg_subst.insert(*name, *ty);
        }
        let raw_return_type = instantiate_call_type(
            self.interner,
            func.return_type,
            &final_subst,
            actual_this_type,
        );
        let raw_return_type = self.hoist_source_placeholders_into_return_type(raw_return_type);
        let return_type =
            self.normalize_inferred_placeholder_type(raw_return_type, &final_arg_subst);
        let return_type =
            self.hoist_resolved_type_params_into_return_type(func, &final_subst, return_type);
        if self.interner.get_display_alias(return_type).is_none()
            && let Some(app_id) =
                crate::visitor::application_id(self.interner.as_type_database(), raw_return_type)
        {
            let app = self.interner.type_application(app_id);
            let mut changed = false;
            let display_args = app
                .args
                .iter()
                .copied()
                .map(|arg| {
                    let evaluated = if crate::visitor::conditional_type_id(
                        self.interner.as_type_database(),
                        arg,
                    )
                    .is_some()
                        || self.application_expands_to_conditional_alias_for_return_display(arg)
                    {
                        self.checker.evaluate_type(arg)
                    } else {
                        arg
                    };
                    changed |= evaluated != arg;
                    evaluated
                })
                .collect::<Vec<_>>();
            if changed || return_type != raw_return_type {
                let display_app = self.interner.application(app.base, display_args);
                self.interner.store_display_alias(return_type, display_app);
                let evaluated_return = self.checker.evaluate_type(return_type);
                if evaluated_return != return_type
                    && self.interner.get_display_alias(evaluated_return).is_none()
                {
                    self.interner
                        .store_display_alias(evaluated_return, display_app);
                }
            }
        }
        // For generic constructor calls (e.g. `new D()` where `class D<T>`),
        // store a display_alias so the formatter shows `D<unknown>` instead of
        // just `D` or the expanded structural type.
        // Guard: skip Application base types to avoid nested Application causing
        // double type args like `Map<K,V><string, number>` for built-in generics.
        if func.is_constructor
            && !func.type_params.is_empty()
            && self.interner.get_display_alias(return_type).is_none()
            && !matches!(
                self.interner.lookup(func.return_type),
                Some(TypeData::Application(_))
            )
        {
            let resolved_args: Vec<TypeId> = func
                .type_params
                .iter()
                .map(|tp| final_subst.get(tp.name).unwrap_or(TypeId::UNKNOWN))
                .collect();
            let app = self.interner.application(func.return_type, resolved_args);
            self.interner.store_display_alias(return_type, app);
        }
        let tracked_final_type_params: FxHashSet<_> =
            func.type_params.iter().map(|tp| tp.name).collect();
        let mut instantiated_params: Vec<ParamInfo> = if final_arg_subst.is_empty() {
            instantiated_params
        } else {
            instantiated_params
                .into_iter()
                .map(|param| {
                    let evaluated = if self.function_like_type_param_appears_in_parameter_position(
                        param.type_id,
                        &tracked_final_type_params,
                    ) {
                        param.type_id
                    } else {
                        let normalized = self
                            .normalize_inferred_placeholder_type(param.type_id, &final_arg_subst);
                        // Evaluate Application types (conditional types) to resolve them
                        // after instantiation, but skip plain type parameters to avoid
                        // infinite loops in self-referential generic inference.
                        if matches!(
                            self.interner.lookup(normalized),
                            Some(TypeData::Application(_))
                        ) {
                            self.interner.evaluate_type(normalized)
                        } else {
                            normalized
                        }
                    };
                    ParamInfo {
                        name: param.name,
                        type_id: evaluated,
                        optional: param.optional,
                        rest: param.rest,
                    }
                })
                .collect()
        };
        if !final_subst.is_empty() {
            for (i, &arg_type) in arg_types.iter().enumerate() {
                let Some(raw_param_type) =
                    self.param_type_for_arg_index(&func.params, i, arg_types.len())
                else {
                    continue;
                };
                let final_param_type =
                    instantiate_type(self.interner, raw_param_type, &final_subst);
                if let Some(expected) = self
                    .conflicting_contextual_signature_instantiation_type(arg_type, final_param_type)
                {
                    return CallResult::ArgumentTypeMismatch {
                        index: i,
                        expected,
                        actual: arg_type,
                        fallback_return: TypeId::ERROR,
                    };
                }
            }
        }
        let final_args: Vec<TypeId> = arg_types
            .iter()
            .enumerate()
            .map(|(i, &arg)| {
                // Preserve spread marker tuples [...T] created by the checker
                // for generic TypeParameter spreads.  These are validated against
                // the full rest param type in check_argument_types_with.
                // Only match markers: a 1-rest-element tuple whose inner type
                // is a TypeParameter (not a regular variadic tuple like [...string[]]).
                if let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(arg) {
                    let elems = self.interner.tuple_list(elems_id);
                    if elems.len() == 1
                        && elems[0].rest
                        && matches!(
                            self.interner.lookup(elems[0].type_id),
                            Some(TypeData::TypeParameter(_))
                        )
                    {
                        return arg;
                    }
                }
                let normalized = if final_arg_subst.is_empty() {
                    arg
                } else {
                    self.normalize_inferred_placeholder_type(arg, &final_arg_subst)
                };
                let Some(param_type) =
                    self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
                else {
                    return normalized;
                };
                if self.has_conflicting_contextual_signature_instantiation(normalized, param_type) {
                    return normalized;
                }
                self.instantiate_generic_function_argument_against_target(normalized, param_type)
            })
            .collect();
        tracing::debug!(
            "Final argument check with {} instantiated params",
            instantiated_params.len()
        );
        for (i, (param, &arg_type)) in instantiated_params
            .iter()
            .zip(final_args.iter())
            .enumerate()
        {
            tracing::debug!("  Param {}: {:?}", i, self.interner.lookup(param.type_id));
            tracing::debug!("  Arg   {}: {:?}", i, self.interner.lookup(arg_type));
        }
        let final_args_len = final_args.len();
        for (i, (param, &arg_type)) in instantiated_params
            .iter_mut()
            .zip(final_args.iter())
            .enumerate()
        {
            let duplicate_constraint = if self.object_constraint_properties_are_any(param.type_id) {
                Some(param.type_id)
            } else {
                self.param_type_for_arg_index(&func.params, i, final_args_len)
                    .and_then(|raw| match self.interner.lookup(raw) {
                        Some(TypeData::TypeParameter(tp)) => tp.constraint,
                        _ => None,
                    })
                    .map(|constraint| {
                        let instantiated = instantiate_call_type(
                            self.interner,
                            constraint,
                            &final_subst,
                            actual_this_type,
                        );
                        self.checker.evaluate_type(instantiated)
                    })
                    .filter(|&constraint| self.object_constraint_properties_are_any(constraint))
            };
            if duplicate_constraint.is_some()
                && let Some(expected) = self.duplicate_single_arg_application_value_shape(arg_type)
            {
                param.type_id = expected;
            }
        }
        // Store instantiated params for post-inference excess property checking.
        // The checker needs these to perform EPC on the concrete (post-inference)
        // parameter types rather than the raw types that still contain type parameters.
        // Store BEFORE the final check so they're available even if the check fails
        // (the checker uses these to perform EPC on ArgumentTypeMismatch too).
        self.apply_callback_optional_rest_slots(func, arg_types, &mut instantiated_params);
        self.last_instantiated_params = Some(instantiated_params.clone());

        if let Some((index, expected, actual)) = first_direct_primitive_mismatch {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return: return_type,
            };
        }

        if let Some(result) = self.generic_rest_tuple_callback_arity_mismatch(func, &final_args) {
            return result;
        }

        if let Some(result) =
            self.check_argument_types_with(&instantiated_params, &final_args, true, func.is_method)
        {
            tracing::debug!("Final check failed: {:?}", result);
            return match result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => {
                    // Check if this parameter's type contains a type parameter that
                    // fell back to its default. If so, skip the error - the default is
                    // a fallback when inference fails, not a constraint.
                    let param_type = self
                        .param_type_for_arg_index(&func.params, index, final_args.len())
                        .unwrap_or(expected);
                    let should_skip = default_fallback_tp_names.iter().any(|&tp_name| {
                        crate::visitors::visitor_predicates::contains_type_parameter_named(
                            self.interner,
                            param_type,
                            tp_name,
                        )
                    });
                    if should_skip {
                        tracing::debug!(
                            "Skipping argument mismatch at index {} - parameter type uses default fallback",
                            index
                        );
                        return CallResult::Success(return_type);
                    }
                    // When the original parameter type is a bare const type parameter
                    // (e.g., `x: T` where T has `const` modifier), skip the argument
                    // mismatch. Const type parameters are inferred directly FROM the
                    // argument type, so the argument is always assignable by construction.
                    // The mismatch arises because the checker computes the arg type with
                    // `in_const_assertion = true` (producing one TypeId) while the solver's
                    // inference engine applies `apply_const_assertion` separately (producing
                    // a different TypeId). Both represent the same readonly/literal type.
                    let is_bare_const_type_param = func.type_params.iter().any(|tp| {
                        tp.is_const
                            && matches!(
                                self.interner.lookup(param_type),
                                Some(TypeData::TypeParameter(info)) if info.name == tp.name
                            )
                    });
                    if is_bare_const_type_param {
                        tracing::debug!(
                            "Skipping argument mismatch at index {} - bare const type parameter",
                            index
                        );
                        return CallResult::Success(return_type);
                    }

                    let expected = self
                        .param_type_for_arg_index(&func.params, index, final_args.len())
                        .and_then(|raw| match self.interner.lookup(raw) {
                            Some(TypeData::TypeParameter(tp)) => {
                                constraint_fallback_display_types.get(&tp.name).copied()
                            }
                            _ => None,
                        })
                        .unwrap_or(expected);

                    CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    }
                }
                _ => result,
            };
        }
        for (i, (&arg_type, raw_param)) in final_args.iter().zip(func.params.iter()).enumerate() {
            if raw_param.rest {
                continue;
            }
            let raw_param_type = raw_param.type_id;
            let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(raw_param_type) else {
                continue;
            };
            let Some(constraint) = tp.constraint else {
                continue;
            };
            if crate::visitors::visitor_predicates::contains_infer_types(
                self.interner.as_type_database(),
                constraint,
            ) {
                continue;
            }
            let constraint =
                instantiate_call_type(self.interner, constraint, &final_subst, actual_this_type);
            if crate::type_queries::contains_type_parameters_db(
                self.interner.as_type_database(),
                constraint,
            ) && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(constraint)
                && tp.constraint.is_none()
            {
                continue;
            }
            if !self.arg_satisfies_type_parameter_constraint(arg_type, constraint)
                && !self.is_function_union_compat(arg_type, constraint)
                && !self.callable_satisfies_top_rest_any_constraint(arg_type, constraint)
            {
                return CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: constraint,
                    actual: arg_type,
                    fallback_return: return_type,
                };
            }
        }

        tracing::debug!("Final check succeeded");

        // Instantiate the type predicate if present, so the checker can use it
        // for flow narrowing with the correct (inferred) type arguments.
        if let Some(ref predicate) = func.type_predicate {
            let instantiated_predicate = TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id: predicate.type_id.map(|tid| {
                    instantiate_call_type(self.interner, tid, &final_subst, actual_this_type)
                }),
                parameter_index: predicate.parameter_index,
            };
            let instantiated_params_for_pred: Vec<ParamInfo> = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_call_type(
                        self.interner,
                        p.type_id,
                        &final_subst,
                        actual_this_type,
                    ),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.last_instantiated_predicate =
                Some((instantiated_predicate, instantiated_params_for_pred));
        }

        CallResult::Success(return_type)
    }

    fn apply_callback_optional_rest_slots(
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

    /// Returns `true` when `ty` is or structurally contains a `TypeParameter` that
    /// does not belong to the current generic call (i.e. is absent from `var_map`).
    ///
    /// "Foreign" covers two cases:
    ///  - A bare `__infer_*` placeholder from an enclosing call scope.
    ///  - The original, user-named `TypeParameter` (e.g. `T`) from the enclosing
    ///    function — which appears when `generic_function_shape_for_inference`
    ///    renames the callee's type params but the argument type still carries
    ///    the outer scope's unsubstituted `TypeParameter`.
    ///
    /// Intrinsic and concrete types (primitives, objects, etc.) are never foreign.
    /// The caller is responsible for ensuring `has_usable_contra_candidates` is
    /// true before using this result, to prevent false positives for independent
    /// generic calls like `identity(value[key])`.
    fn type_contains_any_foreign_type_param(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            // Any TypeParameter not registered in this call's var_map is foreign.
            Some(TypeData::TypeParameter(_)) => !var_map.contains_key(&ty),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .any(|&m| self.type_contains_any_foreign_type_param(m, var_map)),
            Some(TypeData::IndexAccess(obj, idx)) => {
                self.type_contains_any_foreign_type_param(obj, var_map)
                    || self.type_contains_any_foreign_type_param(idx, var_map)
            }
            Some(TypeData::Array(elem)) => self.type_contains_any_foreign_type_param(elem, var_map),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_any_foreign_type_param(app.base, var_map)
                    || app
                        .args
                        .iter()
                        .any(|&a| self.type_contains_any_foreign_type_param(a, var_map))
            }
            _ => false,
        }
    }

    fn application_expands_to_conditional_alias_for_return_display(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        if !matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Application(_))
        ) {
            return false;
        }
        self.checker
            .expand_type_alias_application(type_id)
            .is_some_and(|expanded| {
                matches!(
                    self.interner.lookup(expanded),
                    Some(TypeData::Conditional(_))
                )
            })
    }
}
