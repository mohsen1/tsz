//! Generic function call inference.
//!
//! Contains the core generic call resolution logic, including:
//! - Multi-pass type argument inference (Round 1 + Round 2)
//! - Contextual type computation for lambda arguments
//! - Trivial single-type-param fast path
//! - Placeholder normalization

use crate::inference::infer::{InferenceContext, InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeInstantiator, TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use crate::{TypeDatabase, contains_type_by_id};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

/// Check if a type constraint is a primitive type (string, number, boolean, bigint)
/// or a union containing a primitive. Used to preserve literal types during inference
/// when the constraint implies literals should be kept (e.g., `T extends string`).
fn constraint_is_primitive_type(interner: &dyn crate::QueryDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::STRING
        || type_id == TypeId::NUMBER
        || type_id == TypeId::BOOLEAN
        || type_id == TypeId::BIGINT
    {
        return true;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type(interner, m))
        }
        // `keyof T` constraints produce string literal unions at runtime,
        // so literals should be preserved (not widened to `string`).
        Some(TypeData::KeyOf(_)) => true,
        // Intersections like `keyof T & string` — check if any member
        // implies literal preservation.
        Some(TypeData::Intersection(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type(interner, m))
        }
        _ => false,
    }
}

fn instantiate_call_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    actual_this_type: Option<TypeId>,
) -> TypeId {
    if substitution.is_empty() || substitution.is_identity(interner) {
        if let Some(actual_this_type) = actual_this_type {
            let mut instantiator = TypeInstantiator::new(interner, substitution);
            instantiator.this_type = Some(actual_this_type);
            instantiator.instantiate(type_id)
        } else {
            type_id
        }
    } else {
        let mut instantiator = TypeInstantiator::new(interner, substitution);
        instantiator.this_type = actual_this_type;
        instantiator.instantiate(type_id)
    }
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn hoist_resolved_type_params_into_return_type(
        &self,
        func: &FunctionShape,
        final_subst: &TypeSubstitution,
        return_type: TypeId,
    ) -> TypeId {
        let Some(TypeData::Function(shape_id)) = self.interner.lookup(return_type) else {
            return return_type;
        };

        let mut shape = self.interner.function_shape(shape_id).as_ref().clone();
        if !shape.type_params.is_empty() {
            return return_type;
        }

        let mut hoisted = Vec::new();
        let mut seen = FxHashSet::default();
        for tp in &func.type_params {
            let Some(resolved) = final_subst.get(tp.name) else {
                continue;
            };
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(resolved) else {
                continue;
            };
            if seen.insert(info.name)
                && crate::contains_type_parameter_named(
                    self.interner.as_type_database(),
                    return_type,
                    info.name,
                )
            {
                hoisted.push(info);
            }
        }

        if hoisted.is_empty() {
            return return_type;
        }

        shape.type_params = hoisted;
        self.interner.function(shape)
    }

    fn normalize_function_shape_params_for_context(&self, shape: &FunctionShape) -> FunctionShape {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut normalized = shape.clone();
        normalized.params = shape
            .params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        normalized
    }

    fn get_overloaded_source_signature_for_arity(
        db: &dyn crate::TypeDatabase,
        type_id: TypeId,
        arg_count: usize,
    ) -> Option<FunctionShape> {
        let signatures = crate::type_queries::get_call_signatures(db, type_id)
            .filter(|signatures| !signatures.is_empty())
            .or_else(|| {
                crate::type_queries::get_construct_signatures(db, type_id)
                    .filter(|signatures| !signatures.is_empty())
            })?;
        let signature_accepts_arg_count = |params: &[crate::types::ParamInfo], count: usize| {
            let required_count = params.iter().filter(|p| !p.optional).count();
            let has_rest = params.iter().any(|p| p.rest);
            if has_rest {
                count >= required_count
            } else {
                count >= required_count && count <= params.len()
            }
        };
        let sig = signatures
            .iter()
            .rev()
            .find(|sig| signature_accepts_arg_count(&sig.params, arg_count))
            .or_else(|| signatures.last())?;
        Some(FunctionShape {
            type_params: sig.type_params.clone(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        })
    }

    fn get_source_signature_for_target(
        db: &dyn crate::TypeDatabase,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Option<(FunctionShape, FunctionShape)> {
        let target_fn = Self::get_contextual_signature(db, target_type)?;
        let source_fn = Self::get_overloaded_source_signature_for_arity(
            db,
            source_type,
            target_fn.params.len(),
        )
        .or_else(|| Self::get_contextual_signature(db, source_type))?;
        Some((source_fn, target_fn))
    }

    fn should_use_contextual_return_substitution(
        &mut self,
        inferred: TypeId,
        contextual: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if matches!(inferred, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters. Outer-scope type parameters (e.g., `U`
        // from an enclosing `function test<U>(...)`) are concrete in this context
        // and should not trigger the contextual return substitution override.
        let mut visited = FxHashSet::default();
        if self.type_contains_placeholder(inferred, var_map, &mut visited)
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
        {
            return true;
        }

        // If the inferred result only reached a broad fallback (typically the
        // declared constraint/default) and the contextual return substitution is
        // strictly narrower, prefer the contextual result. This keeps round-2
        // contextual typing from being discarded for deferred callback arguments.
        if self.checker.is_assignable_to(contextual, inferred)
            && !self.checker.is_assignable_to(inferred, contextual)
        {
            return true;
        }

        false
    }

    fn contains_tuple_like_parameter_target(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
        if crate::type_queries::get_tuple_elements(db, type_id).is_some() {
            return true;
        }

        if let Some(members) = crate::type_queries::get_union_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        if let Some(members) = crate::type_queries::get_intersection_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        false
    }

    fn can_apply_contextual_return_substitution(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
        inferred: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        let has_non_return_candidates =
            infer_ctx.var_has_candidates(var) && !infer_ctx.all_candidates_are_return_type(var);

        if !has_non_return_candidates {
            return true;
        }

        if matches!(inferred, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters.
        let mut visited = FxHashSet::default();
        self.type_contains_placeholder(inferred, var_map, &mut visited)
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
    }

    fn collect_return_context_substitution(
        &self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }

        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && substitution.get(tp.name).is_none()
        {
            substitution.insert(tp.name, target);
            return;
        }

        if let Some(target_members) =
            crate::type_queries::get_union_members(self.interner.as_type_database(), target)
        {
            let before_len = substitution.len();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if substitution.len() > before_len {
                    return;
                }
            }
        }

        if let Some(inner) = match self.interner.lookup(target) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                source,
                inner,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(inner) = match self.interner.lookup(source) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        let source_eval = self.interner.evaluate_type(source);
        let target_eval = self.interner.evaluate_type(target);
        let function_info = match (
            Self::get_contextual_signature(self.interner.as_type_database(), source),
            Self::get_contextual_signature(self.interner.as_type_database(), target),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                Self::get_contextual_signature(self.interner.as_type_database(), source_eval),
                Self::get_contextual_signature(self.interner.as_type_database(), target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        if let Some((source_fn, target_fn)) = function_info
            && source_fn.params.len() <= target_fn.params.len()
        {
            for (source_param, target_param) in source_fn.params.iter().zip(target_fn.params.iter())
            {
                self.collect_return_context_substitution(
                    source_param.type_id,
                    target_param.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            self.collect_return_context_substitution(
                source_fn.return_type,
                target_fn.return_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let (Some(TypeData::Tuple(source_list_id)), Some(TypeData::Tuple(target_list_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let source_elems = self.interner.tuple_list(source_list_id);
            let target_elems = self.interner.tuple_list(target_list_id);
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_substitution(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        if let (Some(source_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source),
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), target),
        ) {
            self.collect_return_context_substitution(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some((_target_base, target_args)) =
                crate::type_queries::get_application_info(self.interner.as_type_database(), target)
            && target_args.len() == 1
        {
            self.collect_return_context_substitution(
                source_elem,
                target_args[0],
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some(iterator_info) =
                crate::operations::get_iterator_info(self.interner, target, false)
        {
            self.collect_return_context_substitution(
                source_elem,
                iterator_info.yield_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        let source_eval = self.interner.evaluate_type(source);
        let target_eval = self.interner.evaluate_type(target);
        let app_info = match (
            crate::type_queries::get_application_info(self.interner.as_type_database(), source),
            crate::type_queries::get_application_info(self.interner.as_type_database(), target),
        ) {
            (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
            _ => match (
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    source_eval,
                ),
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    target_eval,
                ),
            ) {
                (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
                _ => None,
            },
        };

        if let Some(((source_base, source_args), (target_base, target_args))) = app_info
            && source_base == target_base
            && source_args.len() == target_args.len()
        {
            for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                self.collect_return_context_substitution(
                    *source_arg,
                    *target_arg,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
        }
    }

    fn compute_return_context_substitution(
        &self,
        func: &FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return TypeSubstitution::new();
        };

        let tracked_type_params: FxHashSet<_> = func.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return TypeSubstitution::new();
        }

        let mut substitution = TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            func.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        );
        substitution
    }

    pub(crate) fn resolve_generic_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);
        let result = self.resolve_generic_call_inner(func, arg_types);
        self.defaulted_placeholders = previous_defaulted;
        result
    }

    fn resolve_generic_call_inner(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
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
        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
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
            // We use a TypeParameter with a special name to track it during constraint collection
            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", var.0).expect("write to String is infallible");
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
        // We suppress widening in two cases:
        // (a) The constraint contains literal types (discriminated union protection)
        // (b) The type parameter is referenced in another type param's constraint,
        //     because widening would cause a mismatch between the widened candidate
        //     and the un-widened contextual type used for callback parameters.
        let widenable_placeholders: FxHashSet<TypeId> = var_map
            .keys()
            .filter(|&&placeholder_id| {
                if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(placeholder_id) {
                    // (a) Skip if constraint implies literal types
                    if let Some(constraint) = tp.constraint {
                        let inst = instantiate_call_type(
                            self.interner,
                            constraint,
                            &substitution,
                            actual_this_type,
                        );
                        if type_implies_literals_deep(self.interner, inst) {
                            return false;
                        }
                    }
                    // (b) Skip if this placeholder is referenced in another type param's
                    // constraint (e.g., TContext in TMethods extends Record<..., (ctx: TContext) => ...>)
                    let is_referenced_in_other_constraints =
                        func.type_params.iter().any(|other_tp| {
                            if other_tp.name == tp.name {
                                return false; // Skip self
                            }
                            if let Some(constraint) = other_tp.constraint {
                                let inst = instantiate_call_type(
                                    self.interner,
                                    constraint,
                                    &substitution,
                                    actual_this_type,
                                );
                                type_references_placeholder(self.interner, inst, placeholder_id)
                            } else {
                                false
                            }
                        });
                    !is_referenced_in_other_constraints
                } else {
                    false
                }
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
            let all_return_vars_covered = !return_is_bare_var
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
                ) && {
                    let mut visited = FxHashSet::default();
                    self.type_contains_placeholder(
                        return_type_with_placeholders,
                        &var_map,
                        &mut visited,
                    )
                };
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
        if has_context_sensitive_args && !structural_return_subst.is_empty() {
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
            let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                && i >= instantiated_params.len().saturating_sub(1);
            let track_direct_placeholder_vars =
                !is_rest_param_arg && !self.type_evaluates_to_function(target_type);

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
                    if track_direct_placeholder_vars {
                        direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                            target_type,
                            &var_map,
                            &mut placeholder_probe_map,
                            &mut placeholder_visited,
                        ));
                    }
                } else {
                    // Target type contains placeholders - check against their constraints
                    if track_direct_placeholder_vars {
                        direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                            target_type,
                            &var_map,
                            &mut placeholder_probe_map,
                            &mut placeholder_visited,
                        ));
                    }
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(target_type)
                        && let Some(constraint) = tp.constraint
                    {
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
                            // Constraint is fully concrete - safe to check now
                            if !self
                                .checker
                                .is_assignable_to(contextual_arg_type, inst_constraint)
                                && !self
                                    .is_function_union_compat(contextual_arg_type, inst_constraint)
                            {
                                return CallResult::ArgumentTypeMismatch {
                                    index: i,
                                    expected: inst_constraint,
                                    actual: contextual_arg_type,
                                    fallback_return: TypeId::ERROR,
                                };
                            }
                        }
                    }
                }
            } else if !is_rest_param_arg {
                direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                    target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                ));
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
            let source_for_inference = self.instantiate_generic_function_argument_against_target(
                source_for_inference,
                contextual_target_type,
            );

            // arg_type <: target_type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                source_for_inference,
                contextual_target_type,
                crate::types::InferencePriority::NakedTypeVariable,
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
                    crate::types::InferencePriority::NakedTypeVariable,
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
                        self.constrain_types(
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
                .is_some_and(|constraints| !constraints.lower_bounds.is_empty());
            if !appears_in_other_params || !has_covariant_candidates {
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
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(resolved) = infer_ctx.probe(var) {
                // This var was fixed in Round 1 — map its placeholder name to the resolved type
                use std::fmt::Write;
                placeholder_buf.clear();
                write!(placeholder_buf, "__infer_{}", var.0)
                    .expect("write to String is infallible");
                let placeholder_atom = self.interner.intern_string(&placeholder_buf);
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

                // Only process contextually sensitive arguments in Round 2
                if !self.is_contextually_sensitive(arg_type) {
                    continue;
                }

                // Check if original target_type contains placeholders BEFORE re-instantiation.
                placeholder_visited.clear();
                let original_has_placeholders =
                    self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited);
                let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                    && i >= instantiated_params.len().saturating_sub(1);
                let round2_target_type = round2_params
                    .as_ref()
                    .and_then(|params| self.param_type_for_arg_index(params, i, arg_types.len()));

                if original_has_placeholders && !is_rest_param_arg {
                    direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                        target_type,
                        &var_map,
                        &mut placeholder_probe_map,
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
                    // Re-instantiate target_type with fixed Round 1 results.
                    // This replaces resolved placeholders with their inferred types while
                    // preserving unresolved placeholders for further Round 2 inference.
                    let r2_target = if let Some(candidate) = round2_target_type {
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
            let ty = if has_constraints {
                let mut resolved_direct = None;
                let contra_only = infer_ctx.has_only_contra_candidates(var);

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
                            let ty = if all_return_type {
                                self.resolve_return_position_inference_type(&lower_bounds, ty)
                            } else if direct_param_vars.contains(&var)
                                && !has_index_signature_candidates
                            {
                                self.resolve_direct_parameter_inference_type(&lower_bounds, ty)
                            } else {
                                ty
                            };
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
                        infer_subst_cache = Some(infer_ctx.get_current_substitution());
                        infer_subst_cache
                            .as_ref()
                            .expect("inference substitution cache just initialized")
                    };
                    self.normalize_inferred_placeholder_type(ty, infer_subst)
                } else if !tp.is_const
                    && !contra_only
                    && infer_ctx.all_candidates_are_fresh_literals(var)
                    && !tp
                        .constraint
                        .is_some_and(|c| constraint_is_primitive_type(self.interner, c))
                {
                    // Only widen when all covariant candidates are fresh literals
                    // (from expressions, not type annotations) AND the type parameter
                    // does NOT have a primitive constraint (string, number, bigint).
                    // tsc preserves literal types when the constraint is a primitive:
                    //   <T extends string>(a: T) => T  -- T="z" preserved
                    //   <T>(a: T) => T                  -- T="z" widened to string
                    crate::widen_literal_type(self.interner.as_type_database(), ty)
                } else {
                    ty
                }
            } else if let Some(default) = tp.default {
                let ty =
                    instantiate_call_type(self.interner, default, &final_subst, actual_this_type);
                trace!(resolved_type = ?ty, "Using default type");
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

            let type_param_name = self.interner.resolve_atom(tp.name);
            let ty = if let Some(contextual_ty) = structural_return_subst.get(tp.name) {
                let can_apply = self.can_apply_contextual_return_substitution(
                    &mut infer_ctx,
                    var,
                    ty,
                    &var_map,
                );
                let should_use =
                    self.should_use_contextual_return_substitution(ty, contextual_ty, &var_map);
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
            let full_subst = infer_ctx.get_current_substitution();
            let mut resolved_subst = TypeSubstitution::new();
            for (name, ty) in final_subst.map().iter() {
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
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(constraint) = tp.constraint {
                let ty = final_subst.get(tp.name).unwrap_or(TypeId::ERROR);
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
                if !self.checker.is_assignable_to(ty_for_check, constraint_ty) {
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
                    // inheritance chains (e.g., DOM). Keep it; upper bounds were validated.
                    if matches!(self.interner.lookup(ty), Some(TypeData::Lazy(_))) {
                        final_subst.insert(tp.name, ty);
                        continue;
                    }
                    // Try to recover using un-widened literal candidates when widening
                    // caused the violation (e.g., "b" widened to string violates keyof O).
                    let un_widened = infer_ctx.get_literal_candidates(var);
                    let recovered = if !un_widened.is_empty() {
                        let candidate_type = if un_widened.len() == 1 {
                            un_widened[0]
                        } else {
                            self.interner.union(un_widened)
                        };
                        if self.checker.is_assignable_to(candidate_type, constraint_ty) {
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
                        // Fall back to constraint type so argument checking emits TS2345
                        final_subst.insert(tp.name, constraint_ty);
                        constraint_fallback_tp_names.insert(tp.name);
                    }
                }
            }
        }

        // Check if the rest param's type parameter was explicitly replaced by
        // its constraint during the fallback path above. This only matches when
        // the constraint check FAILED and the code fell through to the fallback
        // (not when the constraint was naturally resolved as the inferred type).
        let rest_param_from_constraint_fallback = func.params.last().is_some_and(|p| {
            if !p.rest {
                return false;
            }
            if let Some(crate::TypeData::TypeParameter(tp_info)) = self.interner.lookup(p.type_id) {
                constraint_fallback_tp_names.contains(&tp_info.name)
            } else {
                false
            }
        });

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
                    use std::fmt::Write;
                    placeholder_buf.clear();
                    write!(placeholder_buf, "__infer_{}", type_param_vars[i].0)
                        .expect("write to String is infallible");
                    let placeholder_atom = self.interner.intern_string(&placeholder_buf);
                    s.insert(placeholder_atom, inferred);
                }
            }
            s
        };
        let mut final_arg_subst = infer_ctx.get_current_substitution();
        for (name, ty) in placeholder_subst.map().iter() {
            final_arg_subst.insert(*name, *ty);
        }
        let raw_return_type = instantiate_call_type(
            self.interner,
            func.return_type,
            &final_subst,
            actual_this_type,
        );
        let return_type =
            self.normalize_inferred_placeholder_type(raw_return_type, &final_arg_subst);
        let return_type =
            self.hoist_resolved_type_params_into_return_type(func, &final_subst, return_type);
        let instantiated_params: Vec<ParamInfo> = if final_arg_subst.is_empty() {
            instantiated_params
        } else {
            instantiated_params
                .into_iter()
                .map(|param| {
                    let normalized =
                        self.normalize_inferred_placeholder_type(param.type_id, &final_arg_subst);
                    // Evaluate Application types (conditional types) to resolve them
                    // after instantiation, but skip plain type parameters to avoid
                    // infinite loops in self-referential generic inference.
                    let evaluated = if matches!(
                        self.interner.lookup(normalized),
                        Some(TypeData::Application(_))
                    ) {
                        self.interner.evaluate_type(normalized)
                    } else {
                        normalized
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
        let final_args: Vec<TypeId> = arg_types
            .iter()
            .enumerate()
            .map(|(i, &arg)| {
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
        // Store instantiated params for post-inference excess property checking.
        // The checker needs these to perform EPC on the concrete (post-inference)
        // parameter types rather than the raw types that still contain type parameters.
        // Store BEFORE the final check so they're available even if the check fails
        // (the checker uses these to perform EPC on ArgumentTypeMismatch too).
        self.last_instantiated_params = Some(instantiated_params.clone());

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
                    let expected = self
                        .param_type_for_arg_index(&func.params, index, final_args.len())
                        .filter(|raw_expected| {
                            crate::type_queries::contains_type_parameters_db(
                                self.interner,
                                *raw_expected,
                            ) && contains_type_by_id(
                                self.interner.as_type_database(),
                                expected,
                                TypeId::UNKNOWN,
                            )
                        })
                        .filter(|raw_expected| {
                            instantiate_call_type(
                                self.interner,
                                *raw_expected,
                                &final_subst,
                                actual_this_type,
                            ) == expected
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

    fn resolve_direct_parameter_inference_type(
        &self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
    ) -> TypeId {
        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let member_list_id = match self.interner.lookup(inferred) {
            Some(TypeData::Union(id)) => id,
            _ => return inferred,
        };

        // If this is already a single-member union, keep it as-is.
        if self.interner.type_list(member_list_id).len() <= 1 {
            return inferred;
        }

        if let Some(preferred_tuple_candidate) =
            self.preferred_specific_tuple_inference_candidate(lower_bounds)
        {
            return preferred_tuple_candidate;
        }

        // Direct arguments should stay narrow when there are heterogeneous candidates.
        // Otherwise TypeScript-style checks can get masked by a broad union result.
        if lower_bounds
            .iter()
            .all(|ty| self.is_mergeable_direct_inference_candidate(*ty))
        {
            // Guard: if lower bounds contain literals with different primitive bases
            // (e.g., "" and 3 → string vs number), fall back to the first candidate.
            // tsc keeps the first candidate in those cases so later argument checks
            // can report a proper TS2345 mismatch.
            if !self.has_conflicting_literal_bases(lower_bounds) {
                return inferred;
            }
        }

        // Fall back to the first lower-bound candidate so later argument checks
        // drive assignability failures on the mismatch site.
        lower_bounds[0]
    }

    fn preferred_specific_tuple_inference_candidate(
        &self,
        lower_bounds: &[TypeId],
    ) -> Option<TypeId> {
        if lower_bounds.len() <= 1
            || !lower_bounds.iter().all(|&ty| {
                crate::type_queries::get_tuple_elements(self.interner.as_type_database(), ty)
                    .is_some()
            })
        {
            return None;
        }

        let mut specific_iter = lower_bounds
            .iter()
            .copied()
            .filter(|&ty| !self.tuple_contains_any_or_unknown(ty));

        if let Some(first) = specific_iter.next()
            && specific_iter.next().is_none()
        {
            // Exactly one specific bound
            return Some(self.sanitize_tuple_inference_candidate(first));
        }

        None
    }

    fn tuple_contains_any_or_unknown(&self, ty: TypeId) -> bool {
        crate::visitor::collect_all_types(self.interner.as_type_database(), ty)
            .into_iter()
            .any(TypeId::is_any_or_unknown)
    }

    fn sanitize_tuple_inference_candidate(&self, ty: TypeId) -> TypeId {
        let mut substitution = TypeSubstitution::new();
        for nested in crate::visitor::collect_all_types(self.interner.as_type_database(), ty) {
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(nested) else {
                continue;
            };
            let replacement = info.constraint.or(info.default).unwrap_or(TypeId::UNKNOWN);
            substitution.insert(info.name, replacement);
        }

        if substitution.is_empty() {
            ty
        } else {
            instantiate_type(self.interner, ty, &substitution)
        }
    }

    fn resolve_return_position_inference_type(
        &self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
    ) -> TypeId {
        let mut concrete_bounds = lower_bounds
            .iter()
            .copied()
            .filter(|ty| {
                !matches!(*ty, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *ty,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *ty,
                    )
            })
            .collect::<Vec<_>>();
        concrete_bounds.dedup();
        if concrete_bounds.len() == 1
            && (crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            ) || matches!(inferred, TypeId::ANY | TypeId::UNKNOWN))
        {
            return concrete_bounds[0];
        }

        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let inferred_union_members = match self.interner.lookup(inferred) {
            Some(TypeData::Union(member_list_id)) => self.interner.type_list(member_list_id),
            _ => return inferred,
        };
        if inferred_union_members.len() <= 1 {
            return inferred;
        }

        let all_structural = lower_bounds
            .iter()
            .all(|ty| self.is_structural_return_inference_candidate(*ty));
        if all_structural {
            return lower_bounds[0];
        }

        inferred
    }

    fn constrain_return_context_structure(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_ty: TypeId,
        target_ty: TypeId,
        priority: crate::types::InferencePriority,
    ) -> bool {
        let mut constrained_structurally = false;
        let raw_apps = match (
            self.interner.lookup(source_ty),
            self.interner.lookup(target_ty),
        ) {
            (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) => {
                Some((s_app_id, t_app_id))
            }
            _ => None,
        };
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let evaluated_apps = match (
            self.interner.lookup(evaluated_source_ty),
            self.interner.lookup(evaluated_target_ty),
        ) {
            (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) => {
                Some((s_app_id, t_app_id))
            }
            _ => None,
        };
        if let Some((s_app_id, t_app_id)) = raw_apps.or(evaluated_apps) {
            let s_app = self.interner.type_application(s_app_id);
            let t_app = self.interner.type_application(t_app_id);
            if s_app.base == t_app.base
                && s_app.args.len() == t_app.args.len()
                && self.should_directly_constrain_same_base_application(source_ty, target_ty)
            {
                constrained_structurally = true;
                for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                    self.constrain_types(infer_ctx, var_map, *s_arg, *t_arg, priority);
                }
            }
        }

        let raw_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        );
        let evaluated_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            evaluated_source_ty,
            evaluated_target_ty,
        );
        if let Some((mut source_fn, target_fn)) = raw_functions.or(evaluated_functions)
            && source_fn.params.len() == target_fn.params.len()
        {
            if !source_fn.type_params.is_empty() {
                let target_param_types: Vec<_> =
                    target_fn.params.iter().map(|p| p.type_id).collect();
                source_fn = self.instantiate_function_shape_from_argument_types(
                    &source_fn,
                    &target_param_types,
                );
            }
            constrained_structurally = true;
            for (source_param, target_param) in source_fn.params.iter().zip(target_fn.params.iter())
            {
                // Function parameters are contravariant in assignability, so the
                // contextual target parameter constrains the returned function's
                // source parameter.
                let nested_structural = self.constrain_return_context_structure(
                    infer_ctx,
                    var_map,
                    target_param.type_id,
                    source_param.type_id,
                    priority,
                );
                if !nested_structural {
                    self.constrain_types(
                        infer_ctx,
                        var_map,
                        target_param.type_id,
                        source_param.type_id,
                        priority,
                    );
                }
            }
            let nested_structural = self.constrain_return_context_structure(
                infer_ctx,
                var_map,
                source_fn.return_type,
                target_fn.return_type,
                priority,
            );
            if !nested_structural {
                self.constrain_types(
                    infer_ctx,
                    var_map,
                    source_fn.return_type,
                    target_fn.return_type,
                    priority,
                );
            }
        }

        constrained_structurally
    }

    fn collect_placeholder_vars_in_type(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        probe_map: &mut FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> FxHashSet<InferenceVar> {
        if var_map.is_empty() {
            return FxHashSet::default();
        }

        let mut result = FxHashSet::default();
        for nested in crate::visitor::collect_all_types(self.interner.as_type_database(), ty) {
            if let Some(&var) = var_map.get(&nested) {
                result.insert(var);
            }
        }
        let evaluated_ty = self.interner.evaluate_type(ty);
        if evaluated_ty != ty {
            for nested in
                crate::visitor::collect_all_types(self.interner.as_type_database(), evaluated_ty)
            {
                if let Some(&var) = var_map.get(&nested) {
                    result.insert(var);
                }
            }
        }
        if result.is_empty() {
            for (&placeholder_id, &var) in var_map.iter() {
                probe_map.clear();
                probe_map.insert(placeholder_id, var);
                visited.clear();
                if self.type_contains_placeholder(ty, probe_map, visited) {
                    result.insert(var);
                }
            }
        }

        result
    }

    fn should_skip_contextual_arg_in_round1(&self, arg_type: TypeId) -> bool {
        if !self.is_contextually_sensitive(arg_type) {
            return false;
        }

        match self.interner.lookup(arg_type) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                !shape
                    .properties
                    .iter()
                    .any(|prop| !self.is_contextually_sensitive(prop.type_id))
            }
            _ => true,
        }
    }

    fn partial_round1_object_pair(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        let source_ty = self.checker.evaluate_type(source_ty);
        let target_ty = self.checker.evaluate_type(target_ty);

        let (Some(source_obj), Some(target_obj)) =
            (
                match self.interner.lookup(source_ty) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
                match self.interner.lookup(target_ty) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
            )
        else {
            return None;
        };

        let source_shape = self.interner.object_shape(source_obj);
        let target_shape = self.interner.object_shape(target_obj);

        let mut target_props_by_name: FxHashMap<_, _> = FxHashMap::default();
        for prop in &target_shape.properties {
            target_props_by_name.insert(prop.name, prop);
        }

        let mut source_properties = Vec::new();
        let mut target_properties = Vec::new();
        for prop in &source_shape.properties {
            if self.is_contextually_sensitive(prop.type_id) {
                continue;
            }

            if let Some(target_prop) = target_props_by_name.get(&prop.name) {
                source_properties.push(prop.clone());
                target_properties.push((**target_prop).clone());
            }
        }

        if source_properties.is_empty() {
            return None;
        }

        if source_properties.len() == source_shape.properties.len()
            && target_properties.len() == target_shape.properties.len()
        {
            return Some((source_ty, target_ty));
        }

        let mut source_shape = (*source_shape).clone();
        source_shape.properties = source_properties;

        let mut target_shape = (*target_shape).clone();
        target_shape.properties = target_properties;

        Some((
            self.interner.object_with_index(source_shape),
            self.interner.object_with_index(target_shape),
        ))
    }

    fn contextual_round1_arg_types(
        &mut self,
        arg_type: TypeId,
        target_type: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        if let (Some(mut source_fn), Some(mut target_fn)) = (
            Self::get_contextual_signature(self.interner.as_type_database(), arg_type),
            Self::get_contextual_signature(self.interner.as_type_database(), target_type),
        ) && source_fn.params.len() == target_fn.params.len()
            && let Some((source_return, target_return)) =
                self.partial_round1_object_pair(source_fn.return_type, target_fn.return_type)
        {
            source_fn.return_type = source_return;
            target_fn.return_type = target_return;
            return Some((
                self.interner.function(source_fn),
                self.interner.function(target_fn),
            ));
        }

        if !self.is_contextually_sensitive(arg_type) {
            return Some((arg_type, target_type));
        }

        if self.should_skip_contextual_arg_in_round1(arg_type) {
            return None;
        }

        let (Some(arg_obj), Some(target_obj)) =
            (
                match self.interner.lookup(arg_type) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
                match self.interner.lookup(target_type) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
            )
        else {
            return Some((arg_type, target_type));
        };

        let arg_shape = self.interner.object_shape(arg_obj);
        let target_shape = self.interner.object_shape(target_obj);

        let mut target_props_by_name: FxHashMap<_, _> = FxHashMap::default();
        for prop in &target_shape.properties {
            target_props_by_name.insert(prop.name, prop);
        }

        let mut arg_properties = Vec::new();
        let mut target_properties = Vec::new();
        for prop in &arg_shape.properties {
            if self.is_contextually_sensitive(prop.type_id) {
                continue;
            }

            if let Some(target_prop) = target_props_by_name.get(&prop.name) {
                arg_properties.push(prop.clone());
                target_properties.push((**target_prop).clone());
            }
        }

        if arg_properties.is_empty() {
            return None;
        }

        if arg_properties.len() == arg_shape.properties.len()
            && target_properties.len() == target_shape.properties.len()
        {
            return Some((arg_type, target_type));
        }

        let mut arg_shape = (*arg_shape).clone();
        arg_shape.properties = arg_properties;

        let mut target_shape = (*target_shape).clone();
        target_shape.properties = target_properties;

        Some((
            self.interner.object_with_index(arg_shape),
            self.interner.object_with_index(target_shape),
        ))
    }

    fn constrain_sensitive_function_return_types(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_ty: TypeId,
        target_ty: TypeId,
        priority: crate::types::InferencePriority,
    ) -> bool {
        let raw_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        );
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let evaluated_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            evaluated_source_ty,
            evaluated_target_ty,
        );

        let Some((mut source_fn, target_fn)) = raw_functions.or(evaluated_functions) else {
            return false;
        };

        if !source_fn.type_params.is_empty() && source_fn.params.len() == target_fn.params.len() {
            let target_param_types: Vec<_> = target_fn.params.iter().map(|p| p.type_id).collect();
            source_fn = self
                .instantiate_function_shape_from_argument_types(&source_fn, &target_param_types);
        }

        if self.is_contextually_sensitive(source_fn.return_type) {
            return false;
        }

        let nested_structural = self.constrain_return_context_structure(
            infer_ctx,
            var_map,
            source_fn.return_type,
            target_fn.return_type,
            priority,
        );
        if !nested_structural {
            self.constrain_types(
                infer_ctx,
                var_map,
                source_fn.return_type,
                target_fn.return_type,
                priority,
            );
        }
        true
    }

    fn instantiate_function_shape_from_argument_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> FunctionShape {
        let substitution = self.compute_contextual_types(func, arg_types);
        FunctionShape {
            params: func
                .params
                .iter()
                .map(|param| ParamInfo {
                    name: param.name,
                    type_id: instantiate_type(self.interner, param.type_id, &substitution),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: instantiate_type(self.interner, func.return_type, &substitution),
            this_type: func
                .this_type
                .map(|this_type| instantiate_type(self.interner, this_type, &substitution)),
            type_params: vec![],
            type_predicate: func.type_predicate.as_ref().map(|predicate| TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id: predicate
                    .type_id
                    .map(|tid| instantiate_type(self.interner, tid, &substitution)),
                parameter_index: predicate.parameter_index,
            }),
            is_constructor: func.is_constructor,
            is_method: func.is_method,
        }
    }

    pub(crate) fn instantiate_generic_function_argument_against_target(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        // Callable types represent class constructor values (e.g., `Promise`).
        // They must not be decomposed into a Function type, because that loses
        // static members and the construct-signature wrapper. This function is
        // designed for inline arrows/lambdas whose generic type params should be
        // instantiated against the target; class values are already concrete.
        if matches!(self.interner.lookup(source_ty), Some(TypeData::Callable(_))) {
            return source_ty;
        }
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let function_info = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        )
        .or_else(|| {
            Self::get_source_signature_for_target(
                self.interner.as_type_database(),
                evaluated_source_ty,
                evaluated_target_ty,
            )
        })
        .or_else(|| {
            // When the target is an Application with a Lazy base (interface-defined
            // callback like `Callback<T, R>`), the solver's evaluate_type can't resolve
            // the Lazy DefId. Use the checker's evaluate_type which has access to the
            // type environment for DefId → Callable resolution.
            let checker_target = self.checker.evaluate_type(target_ty);
            if checker_target != target_ty && checker_target != evaluated_target_ty {
                Self::get_source_signature_for_target(
                    self.interner.as_type_database(),
                    source_ty,
                    checker_target,
                )
            } else {
                None
            }
        });

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        let source_fn = self.normalize_function_shape_params_for_context(&source_fn);
        let target_fn = self.normalize_function_shape_params_for_context(&target_fn);
        if source_fn.type_params.is_empty() {
            let source_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let source_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            if !source_has_calls
                && source_has_constructs
                && !target_has_calls
                && target_has_constructs
            {
                return source_ty;
            }
            return self.interner.function(source_fn);
        }

        let mut target_param_types = Vec::with_capacity(source_fn.params.len());
        for index in 0..source_fn.params.len() {
            let Some(param_type) =
                self.param_type_for_arg_index(&target_fn.params, index, source_fn.params.len())
            else {
                return source_ty;
            };
            target_param_types.push(param_type);
        }

        if target_param_types.is_empty() {
            return source_ty;
        }
        if target_param_types.iter().any(|&param_type| {
            Self::contains_tuple_like_parameter_target(self.interner.as_type_database(), param_type)
        }) {
            return source_ty;
        }
        let source_type_params_fully_determined_by_params =
            source_fn.type_params.iter().all(|tp| {
                source_fn.params.iter().any(|param| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        param.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| info.name == tp.name)
                    })
                })
            });
        let prev_contextual_type = self.contextual_type;
        // Suppress contextual type when source type params are fully determined by params.
        // This prevents return type from incorrectly constraining T when T already comes
        // from param positions (e.g., `identity<T>(v:T)=>T` vs `Iterator<S, boolean>`).
        self.contextual_type = if source_type_params_fully_determined_by_params {
            None
        } else {
            Some(target_ty)
        };
        let instantiated =
            self.instantiate_function_shape_from_argument_types(&source_fn, &target_param_types);
        self.contextual_type = prev_contextual_type;
        self.interner.function(instantiated)
    }

    fn single_concrete_upper_bound(
        &self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
    ) -> Option<TypeId> {
        let constraints = infer_ctx.get_constraints(var)?;
        let mut concrete_upper_bounds = constraints
            .upper_bounds
            .iter()
            .copied()
            .filter(|upper| {
                !matches!(*upper, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *upper,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *upper,
                    )
            })
            .collect::<Vec<_>>();
        concrete_upper_bounds.dedup();
        if concrete_upper_bounds.len() == 1 {
            concrete_upper_bounds.pop()
        } else {
            None
        }
    }

    fn is_mergeable_direct_inference_candidate(&self, ty: TypeId) -> bool {
        let evaluated_ty = self.interner.evaluate_type(ty);
        // Primitives (null, undefined, string, number, boolean, void, never, etc.)
        // are always safe to merge into a union — they don't indicate structural
        // ambiguity. Without this, `equal(B, D | undefined)` would discard the
        // union and use only the first candidate, causing false TS2345 errors.
        if ty.is_nullish() || ty.is_any_or_unknown() || ty == TypeId::NEVER || ty == TypeId::VOID {
            return true;
        }
        // Primitive base types are safe to merge — they're just as unambiguous as
        // null/undefined. Literal types (string/number/boolean/bigint literals)
        // are also safe since they widen to their base primitive during resolution.
        if matches!(
            ty,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::OBJECT
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) {
            return true;
        }
        // Nominal private brands should never be merged into a union during
        // direct argument inference. TypeScript fixes `T` to the first such
        // candidate and reports the later mismatch (`C` vs `D`) instead of
        // inferring `C | D`.
        if crate::type_queries::get_private_brand_name(self.interner.as_type_database(), ty)
            .is_some()
            || crate::type_queries::get_private_field_name(self.interner.as_type_database(), ty)
                .is_some()
            || crate::type_queries::get_private_brand_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
            || crate::type_queries::get_private_field_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
        {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Literal(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_)
                | TypeData::Enum(..)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(..)
                | TypeData::TemplateLiteral(_)
                | TypeData::ReadonlyType(_)
                | TypeData::KeyOf(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_mergeable_direct_inference_candidate(*member))
            }
            _ => false,
        }
    }

    fn is_structural_return_inference_candidate(&self, ty: TypeId) -> bool {
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_structural_return_inference_candidate(*member))
            }
            _ => false,
        }
    }

    /// Returns `true` when the lower bounds contain literal types from different
    /// primitive families (e.g., a string literal and a number literal). This indicates
    /// heterogeneous candidates that tsc would NOT merge into a union.
    fn has_conflicting_literal_bases(&self, lower_bounds: &[TypeId]) -> bool {
        // Direct-parameter inference should keep the leftmost candidate when
        // fresh candidates disagree on primitive base. That preserves TypeScript's
        // first-wins behavior for cases like `bar<T>(x: T, y: T); bar(1, "")`,
        // where `T` should settle on `number` and the second argument should
        // still produce TS2345 instead of broadening the call to `number | string`.
        let mut seen_base: Option<TypeId> = None;
        for &ty in lower_bounds {
            let base = self.primitive_base_of(ty);
            if let Some(b) = base {
                match seen_base {
                    None => seen_base = Some(b),
                    Some(prev) if prev != b => return true,
                    _ => {}
                }
            }
        }
        false
    }

    /// Returns the primitive base TypeId for a type if it's a literal or primitive,
    /// or `None` for non-primitive types (objects, arrays, etc.).
    fn primitive_base_of(&self, ty: TypeId) -> Option<TypeId> {
        // Check well-known primitive TypeIds first
        if matches!(
            ty,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            return Some(ty);
        }
        if matches!(ty, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE) {
            return Some(TypeId::BOOLEAN);
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Literal(lit)) => Some(lit.primitive_type_id()),
            _ => None,
        }
    }

    /// Fast path for identity-style generic calls:
    /// `<T extends C>(x: T) => T` with a single non-rest argument.
    ///
    /// This shape is common in constraint-heavy code and does not require full
    /// multi-pass inference machinery. We can infer `T` directly from the argument,
    /// validate the constraint once, and return the argument type.
    fn resolve_trivial_single_type_param_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        if func.type_params.len() != 1 || func.params.len() != 1 || arg_types.len() != 1 {
            return None;
        }
        if func.params[0].rest || func.params[0].optional {
            return None;
        }
        if func.this_type.is_some() || func.type_predicate.is_some() {
            return None;
        }

        let tp = &func.type_params[0];
        let param_ty = func.params[0].type_id;
        let return_ty = func.return_type;

        let is_tp = |ty: TypeId| {
            matches!(
                self.interner.lookup(ty),
                Some(TypeData::TypeParameter(info)) if info.name == tp.name
            )
        };
        if !is_tp(param_ty) || !is_tp(return_ty) {
            return None;
        }

        // Bail out for self-referential constraints like `T extends Test<keyof T>`.
        // The fast path cannot properly instantiate the constraint with the inferred
        // type (it checks the raw constraint), and it uses `widen_type` which
        // deep-widens object properties. The normal inference path handles this
        // correctly by instantiating the constraint with `final_subst`.
        if let Some(constraint) = tp.constraint
            && crate::visitor::contains_type_parameter_named(
                self.interner.as_type_database(),
                constraint,
                tp.name,
            )
        {
            return None;
        }

        let arg_ty = arg_types[0];
        let inferred_ty = if tp.is_const {
            arg_ty
        } else {
            // When the declared constraint is a primitive type (string, number,
            // boolean, bigint) or a union thereof, preserve literal types without
            // widening. This matches tsc's getInferredType which checks
            // isLiteralType(constraint) and skips getWidenedLiteralType.
            // Example: `<T extends string>(x: T): T` called with `"hello"`
            // should infer T = "hello", not T = string.
            let constraint_is_primitive = tp
                .constraint
                .is_some_and(|c| constraint_is_primitive_type(self.interner, c));
            if constraint_is_primitive {
                arg_ty
            } else {
                let widened = crate::operations::widening::widen_type(
                    self.interner.as_type_database(),
                    arg_ty,
                );
                // When a constraint exists and the widened type violates it, fall back
                // to the unwidened (literal) type. This matches tsc's behavior: for
                // `<T extends 'a' | 'b'>(x: T)` called with `'a'`, T infers as `"a"`
                // (not `string`), because widening to `string` would violate the
                // constraint. Similarly for tuple constraints like
                // `<T extends [string, string, 'a' | 'b']>(x: T)`.
                if let Some(constraint) = tp.constraint {
                    if !self.checker.is_assignable_to(widened, constraint)
                        && self.checker.is_assignable_to(arg_ty, constraint)
                    {
                        arg_ty
                    } else {
                        widened
                    }
                } else {
                    widened
                }
            }
        };
        if let Some(constraint) = tp.constraint
            && !self.checker.is_assignable_to(inferred_ty, constraint)
            && !self.is_function_union_compat(inferred_ty, constraint)
        {
            // In the trivial single-type-param fast path, the parameter IS the
            // type parameter itself, so a constraint violation means the argument
            // doesn't match the effective parameter type (the constraint).
            // tsc reports TS2345 ("Argument of type X is not assignable to
            // parameter of type Y") here, not TS2322.
            return Some(CallResult::ArgumentTypeMismatch {
                index: 0,
                expected: constraint,
                actual: inferred_ty,
                fallback_return: inferred_ty,
            });
        }

        Some(CallResult::Success(inferred_ty))
    }

    /// Collapse transient inference placeholders (like `__infer_src_*`) to stable types.
    ///
    /// The generic call pipeline uses temporary type parameter placeholders for
    /// contextually-instantiated callback arguments. If one of those placeholders
    /// survives as an inferred result, we normalize it through the current
    /// substitution map and fall back to `unknown` if it remains unresolved.
    ///
    /// Uses iterative `instantiate_type` to resolve placeholders within compound
    /// types (e.g., `Array(__infer_src_0)` → `Array(__infer_0)` → `Array(number)`).
    fn normalize_inferred_placeholder_type(
        &self,
        ty: TypeId,
        infer_subst: &TypeSubstitution,
    ) -> TypeId {
        if infer_subst.is_empty() {
            return ty;
        }

        // Iteratively apply substitution to resolve transitive placeholders.
        // Each pass may resolve one level (e.g., __infer_src_0 → __infer_0[] → number[]).
        let mut current = ty;
        for _ in 0..8 {
            let next = instantiate_type(self.interner, current, infer_subst);
            if next == current {
                break;
            }
            current = next;
        }

        let mut source_placeholder_subst = TypeSubstitution::new();
        for ty in crate::visitor::collect_all_types(self.interner.as_type_database(), current) {
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(ty)
                && self
                    .interner
                    .resolve_atom(info.name)
                    .as_str()
                    .starts_with("__infer_src_")
            {
                source_placeholder_subst.insert(info.name, TypeId::UNKNOWN);
            }
        }
        if !source_placeholder_subst.is_empty() {
            current = instantiate_type(self.interner, current, &source_placeholder_subst);
        }

        self.prune_placeholder_union_members(current)
    }

    fn prune_placeholder_union_members(&self, ty: TypeId) -> TypeId {
        let Some(TypeData::Union(member_list_id)) = self.interner.lookup(ty) else {
            return ty;
        };

        let members = self.interner.type_list(member_list_id);
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                !crate::type_queries::contains_infer_types_db(
                    self.interner.as_type_database(),
                    *member,
                )
            })
            .collect();

        if retained.is_empty() || retained.len() == members.len() {
            return ty;
        }

        if retained.len() == 1 {
            retained[0]
        } else {
            self.interner.union_preserve_members(retained)
        }
    }

    /// Computes contextual types for function parameters after Round 1 inference.
    ///
    /// This is used by the Checker to implement two-pass argument checking:
    /// 1. Checker checks non-contextual arguments (arrays, primitives)
    /// 2. Checker calls this method to run Round 1 inference on those arguments
    /// 3. This method returns the current type substitution (with fixed variables)
    /// 4. Checker uses the substitution to construct contextual types for lambdas
    /// 5. Checker checks lambdas with those contextual types (Round 2)
    ///
    /// # Arguments
    /// * `func` - The function shape being called
    /// * `arg_types` - The types of all arguments (both contextual and non-contextual)
    ///
    /// # Returns
    /// A `TypeSubstitution` mapping type parameter placeholder names to their
    /// inferred types after Round 1 inference. The Checker can use this to
    /// instantiate parameter types for contextual arguments.
    pub fn compute_contextual_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> TypeSubstitution {
        use crate::types::InferencePriority;
        let has_context_sensitive_args = arg_types
            .iter()
            .copied()
            .any(|arg| self.is_contextually_sensitive(arg));

        // Save state to prevent pollution if evaluator is reused
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);

        let mut infer_ctx = InferenceContext::with_resolver(
            self.interner.as_type_database(),
            self.interner.as_type_resolver(),
        );
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        self.constraint_recursion_depth.set(0);
        self.constraint_step_count.set(0);

        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
        let mut placeholder_visited = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", var.0).expect("write to String is infallible");
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

            // Track defaulted placeholders to prevent union inference in constrain_types
            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }

            // NOTE: We intentionally do NOT add the type parameter constraint as an
            // upper bound here. The constraint is already part of the TypeParameter
            // declaration and is used as a fallback in Pass 2 (when no inference
            // candidates or upper bounds exist). Adding it as an upper bound during
            // inference initialization would pollute the contextual substitution:
            // when the return type context provides a specific type (e.g.,
            // `(a: number) => void`), creating an intersection with the constraint
            // (e.g., `(...args: any[]) => any`) produces a merged Callable whose
            // conflicting parameter types cause `get_parameter_type` to return None,
            // triggering false TS7006 errors.
        }

        // 2. Instantiate parameters with placeholders
        let mut instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let mut round1_direct_seed_vars = FxHashSet::default();
        for (i, &arg_type) in arg_types.iter().enumerate() {
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
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

        // 2.5. Seed contextual constraints from return type
        // Skip `any` and `unknown` — they don't contribute useful inference constraints
        if !has_context_sensitive_args
            && let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            let return_seed_vars = self.collect_placeholder_vars_in_type(
                return_type_with_placeholders,
                &var_map,
                &mut placeholder_probe_map,
                &mut placeholder_visited,
            );
            // Same logic as primary resolve path: skip only when ALL return vars
            // are covered by round-1 inference.
            let all_return_vars_covered = !return_seed_vars.is_empty()
                && return_seed_vars
                    .iter()
                    .all(|var| round1_direct_seed_vars.contains(var));
            if !all_return_vars_covered {
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );

                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );
            }
        }

        let structural_return_subst = if has_context_sensitive_args {
            TypeSubstitution::new()
        } else {
            // When the source function's return type is a bare type parameter
            // that also appears in its parameter list (like `f<T>(x: T): T`),
            // and the contextual type is a function (from
            // instantiate_generic_function_argument_against_target), extract
            // the function's RETURN TYPE for return context substitution.
            // Without this, `f<T>(x: T): T` passed as `(x: number) => number`
            // would substitute T → (x: number) => number (the full function type)
            // instead of T → number (the return type), causing false TS2322.
            let return_type_is_param_shared_with_params = func.type_params.iter().any(|tp| {
                let ret_is_this_param = matches!(
                    self.interner.lookup(func.return_type),
                    Some(TypeData::TypeParameter(ref info)) if info.name == tp.name
                );
                let param_uses_this_param = func.params.iter().any(|p| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        p.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| info.name == tp.name)
                    })
                });
                ret_is_this_param && param_uses_this_param
            });

            let ctx_for_return = if return_type_is_param_shared_with_params {
                self.contextual_type.map(|ctx| {
                    // Only extract return type from CONCRETE function types
                    // (no type parameters). When the contextual is generic
                    // (e.g. (x: U) => T from outer inference), keep the full
                    // function type so inner inference sees the structure.
                    if crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        ctx
                    } else if let Some(fn_shape) = crate::type_queries::get_function_shape(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        fn_shape.return_type
                    } else {
                        ctx
                    }
                })
            } else {
                self.contextual_type
            };
            self.compute_return_context_substitution(func, ctx_for_return)
        };
        if !structural_return_subst.is_empty() {
            for (&name, &ty) in structural_return_subst.map().iter() {
                substitution.insert(name, ty);
            }
            instantiated_params = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &substitution),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
        }

        // 3. Round 1: Process non-contextual arguments only
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            let Some((contextual_arg_type, contextual_target_type)) =
                self.contextual_round1_arg_types(arg_type, target_type)
            else {
                self.constrain_sensitive_function_return_types(
                    &mut infer_ctx,
                    &var_map,
                    arg_type,
                    target_type,
                    InferencePriority::NakedTypeVariable,
                );
                continue;
            };

            // Add constraint for non-contextual arguments
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                contextual_arg_type,
                contextual_target_type,
                InferencePriority::NakedTypeVariable,
            );

            let source_is_function = self.type_evaluates_to_function(contextual_arg_type);
            let target_is_function = self.type_evaluates_to_function(contextual_target_type);
            if source_is_function || target_is_function {
                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    contextual_arg_type,
                    contextual_target_type,
                    InferencePriority::NakedTypeVariable,
                );
            }

            if let (
                Some(TypeData::Application(arg_app_id)),
                Some(TypeData::Application(target_app_id)),
            ) = (
                self.interner.lookup(contextual_arg_type),
                self.interner.lookup(contextual_target_type),
            ) {
                let arg_app = self.interner.type_application(arg_app_id);
                let target_app = self.interner.type_application(target_app_id);
                if arg_app.base == target_app.base
                    && arg_app.args.len() == target_app.args.len()
                    && self.should_directly_constrain_same_base_application(
                        contextual_arg_type,
                        contextual_target_type,
                    )
                {
                    for (arg_inner, target_inner) in arg_app.args.iter().zip(target_app.args.iter())
                    {
                        self.constrain_types(
                            &mut infer_ctx,
                            &var_map,
                            *arg_inner,
                            *target_inner,
                            InferencePriority::NakedTypeVariable,
                        );
                    }
                }
            }
        }

        // Process rest tuple in Round 1
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // 4. Fix variables with enough information from Round 1
        let _ = infer_ctx.fix_current_variables_with(Some(|source, target| {
            self.checker.is_assignable_to(source, target)
        }));

        // Restore state to prevent pollution if evaluator is reused
        self.defaulted_placeholders = previous_defaulted;

        // 5. Remap substitution to use original type parameter names.
        // get_current_substitution() returns keys like "__infer_0", but the Checker
        // needs keys matching the original type parameter names (e.g., "T", "U")
        // so that instantiate_type can find and replace TypeParameter nodes.
        let infer_subst = infer_ctx.get_current_substitution();
        let mut result_subst = TypeSubstitution::new();

        // Pass 1: Collect all resolved (non-UNKNOWN) type parameters
        let mut unresolved_indices = Vec::new();
        for (i, tp) in func.type_params.iter().enumerate() {
            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", type_param_vars[i].0)
                .expect("write to String is infallible");
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            // Skip the preferred_lower_bound optimization in compute_contextual_types.
            // Unlike resolve_generic_call_inner (which gates this on direct_param_vars
            // for parameters where the type IS the type parameter, like f<T>(x: T)),
            // compute_contextual_types lacks that tracking. Applying it unconditionally
            // can remove genuine candidates that happen to match the constraint type,
            // leading to false positives when the constraint is also a valid candidate
            // from object property inference.
            let preferred_lower_bound: Option<TypeId> = None;
            let resolved = preferred_lower_bound.or_else(|| {
                match infer_ctx.resolve_with_constraints_by(type_param_vars[i], |source, target| {
                    self.checker.is_assignable_to_strict(source, target)
                }) {
                    Ok(resolved) => Some(resolved),
                    Err(_) => self
                        .single_concrete_upper_bound(&mut infer_ctx, type_param_vars[i])
                        .or_else(|| infer_subst.get(placeholder_atom)),
                }
            });
            if let Some(resolved) = resolved {
                let resolved = self.normalize_inferred_placeholder_type(resolved, &infer_subst);
                let resolved = if !has_context_sensitive_args
                    && let Some(contextual_ty) = structural_return_subst.get(tp.name)
                {
                    if self.can_apply_contextual_return_substitution(
                        &mut infer_ctx,
                        type_param_vars[i],
                        resolved,
                        &var_map,
                    ) && self.should_use_contextual_return_substitution(
                        resolved,
                        contextual_ty,
                        &var_map,
                    ) {
                        contextual_ty
                    } else {
                        resolved
                    }
                } else {
                    resolved
                };
                if resolved != TypeId::UNKNOWN {
                    result_subst.insert(tp.name, resolved);
                } else {
                    unresolved_indices.push(i);
                }
            } else {
                unresolved_indices.push(i);
            }
        }

        // Pass 2: For unresolved type params, try using the default or constraint
        // instantiated with already-resolved params as a contextual type.
        // Priority: default > constraint > placeholder (the default is what the type IS
        // when no argument is provided; the constraint is just an upper bound).
        // As a last resort, use the inference placeholder (__infer_N) so that callbacks
        // get unique placeholder types instead of the callee's raw type parameters,
        // which avoids name collisions with outer scope type parameters of the same name.
        for i in unresolved_indices {
            let tp = &func.type_params[i];
            // Try default first — this determines the contextual type when no inference
            // happened (e.g. `<T = TypegenDisabled>` should use TypegenDisabled, not the
            // constraint `TypegenEnabled | TypegenDisabled`).
            if let Some(default) = tp.default {
                let inst_default = instantiate_type(self.interner, default, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_default,
                ) {
                    result_subst.insert(tp.name, inst_default);
                    continue;
                }
            }
            // Fall back to constraint if default didn't resolve.
            // This enables contextual typing for patterns like:
            //   test<TContext, TFn extends (ctx: TContext) => void>(context: TContext, fn: TFn)
            // where TContext is inferred in Round 1 but TFn needs its constraint.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_constraint,
                ) {
                    result_subst.insert(tp.name, inst_constraint);
                    continue;
                }
            }
            // Last resort: use the inference placeholder so callbacks get unique
            // placeholder types instead of the callee's raw type parameter.
            // This ensures that `foo((x) => 1, (x) => '')` produces arg types with
            // `__infer_0` instead of `T`, avoiding name collisions with outer `T`.
            {
                use std::fmt::Write;
                placeholder_buf.clear();
                write!(placeholder_buf, "__infer_{}", type_param_vars[i].0)
                    .expect("write to String is infallible");
                let placeholder_atom = self.interner.intern_string(&placeholder_buf);
                let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                    is_const: tp.is_const,
                    name: placeholder_atom,
                    constraint: tp.constraint,
                    default: None,
                });
                let placeholder_id = self.interner.intern(placeholder_key);
                result_subst.insert(tp.name, placeholder_id);
            }
        }

        result_subst
    }
}

/// Check if a type contains literal types — recursing into unions, intersections,
/// and object properties. Used to detect discriminated union constraints like
/// `{ kind: "a" } | { kind: "b" }` where the literal property types should
/// prevent widening of the corresponding argument properties.
fn type_implies_literals_deep(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape
                .properties
                .iter()
                .any(|prop| type_implies_literals_deep(db, prop.type_id))
        }
        _ => false,
    }
}

/// Check if a type structurally contains a reference to a specific placeholder TypeId.
/// Used to detect when a type parameter (e.g., `TContext`) is referenced inside another
/// type parameter's constraint (e.g., `TMethods` extends Record<string, (ctx: `TContext`) => unknown>).
fn type_references_placeholder(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
    placeholder: TypeId,
) -> bool {
    if type_id == placeholder {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| type_references_placeholder(db, m, placeholder))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                type_references_placeholder(db, prop.type_id, placeholder)
                    || type_references_placeholder(db, prop.write_type, placeholder)
            })
        }
        Some(TypeData::Array(elem)) => type_references_placeholder(db, elem, placeholder),
        Some(TypeData::Tuple(list_id)) => {
            let elems = db.tuple_list(list_id);
            elems
                .iter()
                .any(|e| type_references_placeholder(db, e.type_id, placeholder))
        }
        Some(TypeData::Function(fn_id)) => {
            let func = db.function_shape(fn_id);
            func.params
                .iter()
                .any(|p| type_references_placeholder(db, p.type_id, placeholder))
                || type_references_placeholder(db, func.return_type, placeholder)
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            type_references_placeholder(db, app.base, placeholder)
                || app
                    .args
                    .iter()
                    .any(|&a| type_references_placeholder(db, a, placeholder))
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.conditional_type(cond_id);
            type_references_placeholder(db, cond.check_type, placeholder)
                || type_references_placeholder(db, cond.extends_type, placeholder)
                || type_references_placeholder(db, cond.true_type, placeholder)
                || type_references_placeholder(db, cond.false_type, placeholder)
        }
        Some(TypeData::IndexAccess(obj, idx)) => {
            type_references_placeholder(db, obj, placeholder)
                || type_references_placeholder(db, idx, placeholder)
        }
        Some(TypeData::KeyOf(inner)) => type_references_placeholder(db, inner, placeholder),
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            type_references_placeholder(db, mapped.template, placeholder)
                || type_references_placeholder(db, mapped.constraint, placeholder)
                || mapped
                    .name_type
                    .is_some_and(|n| type_references_placeholder(db, n, placeholder))
        }
        _ => false,
    }
}
