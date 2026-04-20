//! Core type constraint walker for generic type inference.
//!
//! Contains the main structural walker (`constrain_types` / `constrain_types_impl`)
//! that collects type constraints when inferring generic type parameters from
//! argument types.

use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::{AssignabilityChecker, CallEvaluator, MAX_CONSTRAINT_RECURSION_DEPTH};
use crate::relations::variance::compute_type_param_variances_with_resolver;
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate, Variance,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

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

    /// Propagate a top-like type (any/unknown) to all inference placeholders
    /// nested inside a target type. This matches tsc's propagationType mechanism
    /// where `inferFromTypes(target, target)` is called with propagationType set
    /// to the source type, so all type parameter positions receive the source.
    pub(super) fn propagate_type_to_placeholders(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        propagation_type: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        // If target is directly a placeholder, add the propagation type as candidate
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, propagation_type, priority);
            return;
        }

        // Recurse into the target structure to find nested placeholders
        let target_key = self.interner.lookup(target);
        match target_key {
            Some(TypeData::Array(elem)) => {
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, elem, priority);
            }
            Some(TypeData::Tuple(elems_id)) => {
                let elems = self.interner.tuple_list(elems_id);
                for elem in elems.iter() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        elem.type_id,
                        priority,
                    );
                }
            }
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
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        prop.type_id,
                        priority,
                    );
                }
                // Walk index signatures — e.g., { [x: string]: T } needs T to get `any`
                if let Some(ref idx) = shape.string_index {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        idx.value_type,
                        priority,
                    );
                }
                if let Some(ref idx) = shape.number_index {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        idx.value_type,
                        priority,
                    );
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        param.type_id,
                        priority,
                    );
                }
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    shape.return_type,
                    priority,
                );
                // Walk type predicate — e.g., `(a: any) => a is T` has T in predicate
                if let Some(ref pred) = shape.type_predicate
                    && let Some(pred_type) = pred.type_id
                {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        pred_type,
                        priority,
                    );
                }
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                // Walk both call and construct signatures
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &sig.params {
                        self.propagate_type_to_placeholders(
                            ctx,
                            var_map,
                            propagation_type,
                            param.type_id,
                            priority,
                        );
                    }
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        sig.return_type,
                        priority,
                    );
                }
                for prop in &shape.properties {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        prop.type_id,
                        priority,
                    );
                }
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                for arg in app.args.iter().copied() {
                    self.propagate_type_to_placeholders(
                        ctx,
                        var_map,
                        propagation_type,
                        arg,
                        priority,
                    );
                }
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    mapped.constraint,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    mapped.template,
                    priority,
                );
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.check_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.extends_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.true_type,
                    priority,
                );
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    cond.false_type,
                    priority,
                );
            }
            Some(TypeData::ReadonlyType(inner)) | Some(TypeData::KeyOf(inner)) => {
                self.propagate_type_to_placeholders(
                    ctx,
                    var_map,
                    propagation_type,
                    inner,
                    priority,
                );
            }
            Some(TypeData::IndexAccess(obj, idx)) => {
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, obj, priority);
                self.propagate_type_to_placeholders(ctx, var_map, propagation_type, idx, priority);
            }
            _ => {
                // No structural match — stop propagation
            }
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
}

mod structural;
